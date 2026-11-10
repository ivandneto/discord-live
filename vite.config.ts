import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // O Tauri vigia o Rust; o Vite vigiando target/ trava em arquivos em uso (EBUSY).
    watch: { ignored: ["**/src-tauri/**", "**/.playwright-mcp/**"] },
  },
  build: { target: "es2021", outDir: "dist", emptyOutDir: true },
});
