import { afterEach, beforeAll } from "vitest";
import { cleanup } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";

// jsdom には WebCrypto が無いため、Tauri の mockIPC が使う getRandomValues を補う
beforeAll(() => {
  if (!globalThis.crypto?.getRandomValues) {
    Object.defineProperty(globalThis, "crypto", {
      value: {
        getRandomValues: (array: Uint8Array) => {
          for (let i = 0; i < array.length; i++) array[i] = Math.floor(Math.random() * 256);
          return array;
        },
      },
    });
  }
});

afterEach(() => {
  cleanup();
  clearMocks();
});
