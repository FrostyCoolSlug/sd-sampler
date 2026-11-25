import path from 'node:path'
import { fileURLToPath } from 'node:url'

import vue from '@vitejs/plugin-vue'
import { defineConfig } from 'vite'
import { viteSingleFile } from 'vite-plugin-singlefile'

// Each property inspector is its own Vite entry, selected by the PI env var.
// The build script (scripts/build.mjs) runs `vite build` once per PI and copies
// the resulting self-contained index.html into that PI's property_inspector/.
const webRoot = path.dirname(fileURLToPath(import.meta.url))

export default defineConfig((config) => {
  const PI = process.env.PI
  if (!PI) throw new Error('Set the PI environment variable (e.g. PI=sample-pad).')

  return {
    root: path.join(webRoot, 'src', 'pi', PI),
    plugins: [vue(), viteSingleFile()],
    resolve: {
      alias: { '@shared': path.join(webRoot, 'src', 'shared') },
    },
    build: {
      outDir: path.join(webRoot, 'dist', PI),
      emptyOutDir: true,
      // Inline every asset (JS/CSS) so the output is one self-contained file.
      assetsInlineLimit: 100_000_000,
      cssCodeSplit: false,
    },
  }
})
