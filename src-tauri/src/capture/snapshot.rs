//! クリップボードの読み取り。OS の呼び出しは `PasteboardReader` の裏に置き、
//! 機密の取りこぼしを防ぐ手順(読めなかったときは記録しない、読み取り中の入れ替わりを捨てる、
//! 一時的な失敗は再試行する、巨大な画像はデコードの前に捨てる)を、偽の実装でテストする。

use std::collections::HashMap;

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};

use super::filter::FORMAT_ZERO_MEANS_EXCLUDE;
use crate::domain::clip::AppRef;
use crate::error::AppError;
use crate::history::MAX_IMAGE_BYTES;

/// macOS で、コピー元アプリの bundle id を書く慣習の型名。
const SOURCE_HINT_FORMAT: &str = "org.nspasteboard.source";

/// 加工されていない PNG が入っている形式の名前。
#[cfg(target_os = "macos")]
const PNG_FORMAT: &str = "public.png";
#[cfg(not(target_os = "macos"))]
const PNG_FORMAT: &str = "PNG";

/// 画像の1辺の上限(px)。これを超えると、デコードしただけで数百 MB のメモリが要るため、読まずに捨てる。
pub const MAX_IMAGE_DIMENSION: u32 = 8192;

/// 読み取りが失敗したときの再試行の間隔(ミリ秒)。
/// Windows では、他のアプリがクリップボードを開いている間は読めないことがあり、少し待てば読める。
pub const RETRY_DELAYS_MS: [u64; 3] = [50, 100, 200];

/// 読み取りの前後で番号が変わっていたときに、読み直す回数の上限。
pub const CONSISTENCY_ATTEMPTS: u32 = 3;

/// ある時点のクリップボードの内容。
pub struct ClipboardSnapshot {
    /// 利用できる形式の名前(機密の型名の判定に使う)
    pub formats: Vec<String>,
    /// 値を見て判定する形式(Windows の `CanIncludeInClipboardHistory` など)の値。形式名をキーにする
    pub format_values: HashMap<String, Vec<u8>>,
    pub text: Option<String>,
    pub image_png: Option<Vec<u8>>,
    /// 画像があったが、大きすぎるため読まなかった
    pub image_oversize: bool,
    /// `org.nspasteboard.source` などから分かったコピー元
    pub source_hint: Option<AppRef>,
}

/// 画像を読んだ結果。
#[derive(Debug, PartialEq, Eq)]
pub enum ImageRead {
    None,
    Png(Vec<u8>),
    /// 寸法またはバイト数が上限を超えている
    TooLarge,
}

/// クリップボードを読む操作(テストでは偽の実装を使う)。
pub trait PasteboardReader {
    /// 内容が変わるたびに変わる番号(macOS は `changeCount`、Windows は `GetClipboardSequenceNumber`)。
    fn sequence(&self) -> u64;
    /// 利用できる形式の名前。取得できなかったときはエラー。
    fn available_formats(&self) -> Result<Vec<String>, AppError>;
    /// 形式を指定してバイト列を読む。
    fn get_buffer(&self, format: &str) -> Result<Vec<u8>, AppError>;
    /// テキストを読む。無い、または不正な UTF-16 などで読めないときは `None`。
    fn text(&self) -> Option<String>;
    /// 画像を PNG で読む。
    fn image(&self) -> Result<ImageRead, AppError>;
}

/// 1回分の内容を読む。機密かどうかの判定に必要な情報(形式の一覧、除外を示す値)が読めなかったときは、
/// 判定できないまま記録することのないよう、エラーにする。
pub fn read_snapshot(reader: &dyn PasteboardReader) -> Result<ClipboardSnapshot, AppError> {
    let formats = reader.available_formats()?;

    let mut format_values = HashMap::new();
    if let Some(name) = formats
        .iter()
        .find(|f| f.eq_ignore_ascii_case(FORMAT_ZERO_MEANS_EXCLUDE))
    {
        format_values.insert(name.clone(), reader.get_buffer(name)?);
    }

    let source_hint = if formats.iter().any(|f| f == SOURCE_HINT_FORMAT) {
        String::from_utf8(reader.get_buffer(SOURCE_HINT_FORMAT)?)
            .ok()
            .map(|s| s.trim_matches('\0').trim().to_string())
            .filter(|s| !s.is_empty())
            .map(|id| AppRef {
                name: id.clone(),
                id,
            })
    } else {
        None
    };

    let text = reader.text();
    // テキストがあればテキストを優先する(画像の変換は重いので、必要なときだけ読む)
    let text_usable = text.as_deref().is_some_and(|t| !t.trim().is_empty());
    let (image_png, image_oversize) = if text_usable {
        (None, false)
    } else {
        match reader.image()? {
            ImageRead::Png(png) => (Some(png), false),
            ImageRead::TooLarge => (None, true),
            ImageRead::None => (None, false),
        }
    };

    Ok(ClipboardSnapshot {
        formats,
        format_values,
        text,
        image_png,
        image_oversize,
        source_hint,
    })
}

/// 読み取りの前後で番号が同じ(= 読んでいる間に内容が入れ替わらなかった)ときだけ、結果を返す。
/// 形式の一覧と本文を別々に読むため、その間に別のアプリがコピーすると、機密でない一覧と機密の本文が
/// 組み合わさってしまう。変わっていたら捨てて読み直し、上限回数でもそろわなければエラーにする
/// (入れ替えた側のコピーは、別の通知として改めて処理される)。
pub fn read_consistent<T>(
    sequence: &dyn Fn() -> u64,
    read: &mut dyn FnMut() -> Result<T, AppError>,
    attempts: u32,
) -> Result<T, AppError> {
    for _ in 0..attempts {
        let before = sequence();
        let value = read()?;
        if sequence() == before {
            return Ok(value);
        }
    }
    Err(AppError::Clipboard(
        "読み取り中にクリップボードの内容が変わりました".to_string(),
    ))
}

/// 失敗したら、`delays_ms` の間隔を空けて読み直す(最初の1回と合わせて `delays_ms.len() + 1` 回まで)。
pub fn retry_with_delays<T>(
    delays_ms: &[u64],
    sleep: &dyn Fn(u64),
    op: &mut dyn FnMut() -> Result<T, AppError>,
) -> Result<T, AppError> {
    let mut result = op();
    for &ms in delays_ms {
        if result.is_ok() {
            break;
        }
        sleep(ms);
        result = op();
    }
    result
}

/// PNG のヘッダー(IHDR)から、画像の幅と高さを読む。PNG として読めなければ `None`。
/// デコードせずに寸法を知るために使う。
pub fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 24 || bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let be32 = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    Some((be32(16), be32(20)))
}

/// 寸法が上限の内側か。
pub fn dimensions_within_limit(width: u32, height: u32) -> bool {
    width <= MAX_IMAGE_DIMENSION && height <= MAX_IMAGE_DIMENSION
}

/// クリップボードにある加工されていない PNG を、デコードの前に確かめる。
/// ヘッダーが読めない場合は `None` を返し、呼び出し側が通常の読み取りに任せる。
fn classify_raw_png(raw: Vec<u8>) -> Option<ImageRead> {
    if raw.len() > MAX_IMAGE_BYTES {
        return Some(ImageRead::TooLarge);
    }
    let (w, h) = png_dimensions(&raw)?;
    Some(if dimensions_within_limit(w, h) {
        ImageRead::Png(raw)
    } else {
        ImageRead::TooLarge
    })
}

/// clipboard-rs による読み取り。
pub struct RsReader {
    ctx: ClipboardContext,
}

impl RsReader {
    pub fn open() -> Result<Self, AppError> {
        ClipboardContext::new()
            .map(|ctx| Self { ctx })
            .map_err(|e| AppError::Clipboard(e.to_string()))
    }
}

impl PasteboardReader for RsReader {
    fn sequence(&self) -> u64 {
        os_sequence()
    }

    fn available_formats(&self) -> Result<Vec<String>, AppError> {
        self.ctx
            .available_formats()
            .map_err(|e| AppError::Clipboard(e.to_string()))
    }

    fn get_buffer(&self, format: &str) -> Result<Vec<u8>, AppError> {
        self.ctx
            .get_buffer(format)
            .map_err(|e| AppError::Clipboard(e.to_string()))
    }

    fn text(&self) -> Option<String> {
        if self.ctx.has(ContentFormat::Text) {
            self.ctx.get_text().ok()
        } else {
            None
        }
    }

    fn image(&self) -> Result<ImageRead, AppError> {
        if let Some(read) = self
            .ctx
            .get_buffer(PNG_FORMAT)
            .ok()
            .and_then(classify_raw_png)
        {
            return Ok(read);
        }
        if !self.ctx.has(ContentFormat::Image) {
            return Ok(ImageRead::None);
        }
        // PNG 以外(macOS の TIFF、Windows の DIB)はヘッダーを自前で読めないため、デコードしたあとの寸法で確かめ、
        // 上限を超えていれば PNG への変換(さらにメモリを使う)をしない
        let err = |e: Box<dyn std::error::Error + Send + Sync>| AppError::Clipboard(e.to_string());
        let image = self.ctx.get_image().map_err(err)?;
        let (w, h) = image.get_size();
        if !dimensions_within_limit(w, h) {
            return Ok(ImageRead::TooLarge);
        }
        let png = image.to_png().map_err(err)?;
        Ok(ImageRead::Png(png.get_bytes().to_vec()))
    }
}

/// クリップボードの内容が変わるたびに変わる番号。
#[cfg(target_os = "macos")]
fn os_sequence() -> u64 {
    use objc2_app_kit::NSPasteboard;
    NSPasteboard::generalPasteboard().changeCount() as u64
}

#[cfg(windows)]
fn os_sequence() -> u64 {
    // SAFETY: 引数なしで、番号を返すだけの API
    u64::from(unsafe { ::windows::Win32::System::DataExchange::GetClipboardSequenceNumber() })
}

#[cfg(not(any(target_os = "macos", windows)))]
fn os_sequence() -> u64 {
    0
}

/// 実機のクリップボードを読む。失敗したら間隔を空けて再試行し、読み取り中の入れ替わりは読み直す。
pub fn read_from_os() -> Result<ClipboardSnapshot, AppError> {
    retry_with_delays(
        &RETRY_DELAYS_MS,
        &|ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
        &mut || {
            let reader = RsReader::open()?;
            read_consistent(
                &|| reader.sequence(),
                &mut || read_snapshot(&reader),
                CONSISTENCY_ATTEMPTS,
            )
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    /// 偽のクリップボード。`None` の項目は、読み取りが失敗する。
    struct Fake {
        formats: Option<Vec<String>>,
        buffers: HashMap<String, Result<Vec<u8>, ()>>,
        text: Option<String>,
        image: RefCell<Option<Result<ImageRead, ()>>>,
        image_calls: Cell<u32>,
    }

    impl Fake {
        fn new(formats: &[&str]) -> Self {
            Self {
                formats: Some(formats.iter().map(|s| s.to_string()).collect()),
                buffers: HashMap::new(),
                text: None,
                image: RefCell::new(None),
                image_calls: Cell::new(0),
            }
        }
    }

    impl PasteboardReader for Fake {
        fn sequence(&self) -> u64 {
            0
        }
        fn available_formats(&self) -> Result<Vec<String>, AppError> {
            self.formats
                .clone()
                .ok_or_else(|| AppError::Clipboard("失敗".into()))
        }
        fn get_buffer(&self, format: &str) -> Result<Vec<u8>, AppError> {
            match self.buffers.get(format) {
                Some(Ok(v)) => Ok(v.clone()),
                _ => Err(AppError::Clipboard("失敗".into())),
            }
        }
        fn text(&self) -> Option<String> {
            self.text.clone()
        }
        fn image(&self) -> Result<ImageRead, AppError> {
            self.image_calls.set(self.image_calls.get() + 1);
            match self.image.borrow_mut().take() {
                Some(Ok(v)) => Ok(v),
                Some(Err(())) => Err(AppError::Clipboard("失敗".into())),
                None => Ok(ImageRead::None),
            }
        }
    }

    #[test]
    fn 形式の一覧を取得できなかったとき記録せずにエラーを返す() {
        let mut fake = Fake::new(&[]);
        fake.formats = None;
        fake.text = Some("ダミー".into());
        assert!(read_snapshot(&fake).is_err());
    }

    #[test]
    fn 除外を示す形式の値を取得できなかったときエラーを返す() {
        let mut fake = Fake::new(&["CanIncludeInClipboardHistory"]);
        fake.text = Some("ダミー".into());
        fake.buffers
            .insert("CanIncludeInClipboardHistory".into(), Err(()));
        assert!(read_snapshot(&fake).is_err());
    }

    #[test]
    fn 除外を示す形式の値は大文字小文字を問わず読む() {
        let mut fake = Fake::new(&["canincludeinclipboardhistory"]);
        fake.text = Some("ダミー".into());
        fake.buffers
            .insert("canincludeinclipboardhistory".into(), Ok(vec![0, 0, 0, 0]));
        let snap = read_snapshot(&fake).unwrap();
        assert_eq!(
            snap.format_values["canincludeinclipboardhistory"],
            vec![0, 0, 0, 0]
        );
    }

    #[test]
    fn コピー元の申告が一覧にあるのに読めないときエラーを返す() {
        let mut fake = Fake::new(&["org.nspasteboard.source"]);
        fake.text = Some("ダミー".into());
        fake.buffers
            .insert("org.nspasteboard.source".into(), Err(()));
        assert!(read_snapshot(&fake).is_err());
    }

    #[test]
    fn コピー元の申告を読んで前後の空白とヌルを除く() {
        let mut fake = Fake::new(&["org.nspasteboard.source"]);
        fake.text = Some("ダミー".into());
        fake.buffers.insert(
            "org.nspasteboard.source".into(),
            Ok(b" com.example.app\0".to_vec()),
        );
        let snap = read_snapshot(&fake).unwrap();
        assert_eq!(snap.source_hint.unwrap().id, "com.example.app");
    }

    #[test]
    fn テキストが使えるとき画像は読まない() {
        let mut fake = Fake::new(&["public.utf8-plain-text"]);
        fake.text = Some("ダミー".into());
        *fake.image.borrow_mut() = Some(Ok(ImageRead::Png(vec![1])));
        let snap = read_snapshot(&fake).unwrap();
        assert!(snap.image_png.is_none());
        assert_eq!(fake.image_calls.get(), 0);
    }

    #[test]
    fn 空白だけのテキストのとき画像を読む() {
        let mut fake = Fake::new(&["public.png"]);
        fake.text = Some("  ".into());
        *fake.image.borrow_mut() = Some(Ok(ImageRead::Png(vec![1, 2])));
        let snap = read_snapshot(&fake).unwrap();
        assert_eq!(snap.image_png, Some(vec![1, 2]));
        assert!(!snap.image_oversize);
    }

    #[test]
    fn 画像が大きすぎるとき読まずに印だけ付ける() {
        let fake = Fake::new(&["public.png"]);
        *fake.image.borrow_mut() = Some(Ok(ImageRead::TooLarge));
        let snap = read_snapshot(&fake).unwrap();
        assert!(snap.image_png.is_none());
        assert!(snap.image_oversize);
    }

    #[test]
    fn 画像の読み取りに失敗したときエラーを返す() {
        let fake = Fake::new(&["public.png"]);
        *fake.image.borrow_mut() = Some(Err(()));
        assert!(read_snapshot(&fake).is_err());
    }

    // ---- 読み取り中の入れ替わり ----

    #[test]
    fn 読み取りの前後で番号が同じなら結果を返す() {
        let reads = Cell::new(0);
        let r = read_consistent(
            &|| 7,
            &mut || {
                reads.set(reads.get() + 1);
                Ok("内容")
            },
            3,
        );
        assert_eq!(r.unwrap(), "内容");
        assert_eq!(reads.get(), 1);
    }

    #[test]
    fn 読み取り中に番号が変わったら捨てて読み直す() {
        let seq = Cell::new(0u64);
        let reads = Cell::new(0);
        let r = read_consistent(
            &|| seq.get(),
            &mut || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    // 1回目の読み取りの最中に、別のアプリがコピーした
                    seq.set(seq.get() + 1);
                }
                Ok(reads.get())
            },
            3,
        );
        assert_eq!(r.unwrap(), 2, "2回目の読み取り結果を返す");
        assert_eq!(reads.get(), 2);
    }

    #[test]
    fn 上限回数読み直しても番号が変わり続けるときエラーを返す() {
        let seq = Cell::new(0u64);
        let reads = Cell::new(0);
        let r: Result<u32, AppError> = read_consistent(
            &|| seq.get(),
            &mut || {
                reads.set(reads.get() + 1);
                seq.set(seq.get() + 1);
                Ok(0)
            },
            3,
        );
        assert!(matches!(r, Err(AppError::Clipboard(_))));
        assert_eq!(reads.get(), 3);
    }

    #[test]
    fn 読み取り自体が失敗したときは読み直さずにそのエラーを返す() {
        let reads = Cell::new(0);
        let r: Result<u32, AppError> = read_consistent(
            &|| 0,
            &mut || {
                reads.set(reads.get() + 1);
                Err(AppError::Clipboard("失敗".into()))
            },
            3,
        );
        assert!(r.is_err());
        assert_eq!(reads.get(), 1);
    }

    // ---- 再試行 ----

    #[test]
    fn 失敗しても間隔を空けて再試行し成功したら止める() {
        let sleeps = RefCell::new(Vec::new());
        let calls = Cell::new(0);
        let r = retry_with_delays(
            &[50, 100, 200],
            &|ms| sleeps.borrow_mut().push(ms),
            &mut || {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err(AppError::Clipboard("失敗".into()))
                } else {
                    Ok(calls.get())
                }
            },
        );
        assert_eq!(r.unwrap(), 3);
        assert_eq!(*sleeps.borrow(), vec![50, 100]);
    }

    #[test]
    fn 最初に成功したら待たない() {
        let sleeps = RefCell::new(Vec::new());
        let r = retry_with_delays(&[50, 100], &|ms| sleeps.borrow_mut().push(ms), &mut || {
            Ok(1)
        });
        assert_eq!(r.unwrap(), 1);
        assert!(sleeps.borrow().is_empty());
    }

    #[test]
    fn 再試行しても失敗し続けたら最後のエラーを返す() {
        let calls = Cell::new(0);
        let r: Result<u32, AppError> = retry_with_delays(&RETRY_DELAYS_MS, &|_| {}, &mut || {
            calls.set(calls.get() + 1);
            Err(AppError::Clipboard(format!("失敗{}", calls.get())))
        });
        assert_eq!(calls.get(), 4);
        assert_eq!(r, Err(AppError::Clipboard("失敗4".into())));
    }

    #[test]
    fn 再試行の間隔は50から200ミリ秒の範囲にある() {
        assert!(RETRY_DELAYS_MS.iter().all(|ms| (50..=200).contains(ms)));
    }

    // ---- 画像の上限 ----

    /// 幅と高さだけを持つ PNG のヘッダー部分(デコードはできないが、寸法の読み取りには十分)。
    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&width.to_be_bytes());
        v.extend_from_slice(&height.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    #[test]
    fn PNGのヘッダーから幅と高さを読む() {
        assert_eq!(png_dimensions(&png_header(640, 480)), Some((640, 480)));
        assert_eq!(png_dimensions(&png_header(1, 70000)), Some((1, 70000)));
    }

    #[test]
    fn PNGでないデータや短すぎるデータは寸法を読めない() {
        assert_eq!(png_dimensions(&[]), None);
        assert_eq!(png_dimensions(b"GIF89a...................."), None);
        assert_eq!(png_dimensions(&png_header(1, 1)[..20]), None);
    }

    #[test]
    fn 上限ちょうどの寸法は読み_超えたら捨てる() {
        assert!(dimensions_within_limit(8192, 8192));
        assert!(!dimensions_within_limit(8193, 10));
        assert!(!dimensions_within_limit(10, 8193));
    }

    #[test]
    fn 加工されていないPNGは寸法とバイト数を見てデコード前に判定する() {
        let ok = png_header(8192, 8192);
        assert_eq!(classify_raw_png(ok.clone()), Some(ImageRead::Png(ok)));
        assert_eq!(
            classify_raw_png(png_header(8193, 100)),
            Some(ImageRead::TooLarge)
        );
        let mut big = png_header(10, 10);
        big.resize(MAX_IMAGE_BYTES + 1, 0);
        assert_eq!(classify_raw_png(big), Some(ImageRead::TooLarge));
        // ヘッダーが読めないものは、通常の読み取りに任せる
        assert_eq!(classify_raw_png(vec![1, 2, 3]), None);
    }

    /// 実機のクリップボードを読む。手元で明示的に実行する。
    #[test]
    #[ignore]
    fn 実機でスナップショットが取れる() {
        assert!(read_from_os().is_ok());
    }
}
