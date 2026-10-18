import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";
import { resolve } from "node:path";

const root = fileURLToPath(new URL(".", import.meta.url));

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "chrome105",
    outDir: "dist",
    rollupOptions: {
      input: {
        main: resolve(root, "index.html"),
        ime: resolve(root, "ime.html"),
      },
    },
  },
});
