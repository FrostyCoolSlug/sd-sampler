// Builds every property inspector under src/pi/* into its own self-contained
// plugin.html, then copies each one to the path OpenDeck loads (from that PI's
// pi.config.json `out`). Adding a new PI is just dropping a new src/pi/<name>/
// folder — it is discovered automatically here.
import { execSync } from 'node:child_process'
import { copyFileSync, mkdirSync, readdirSync, readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptsDir = path.dirname(fileURLToPath(import.meta.url))
const webRoot = path.dirname(scriptsDir)
const piRoot = path.join(webRoot, 'src', 'pi')

const pis = readdirSync(piRoot, { withFileTypes: true })
  .filter((e) => e.isDirectory())
  .map((e) => e.name)

if (pis.length === 0) throw new Error(`No property inspectors found in ${piRoot}`)

for (const pi of pis) {
  execSync('npx vite build', { stdio: 'inherit', cwd: webRoot, env: { ...process.env, PI: pi } })

  const cfg = JSON.parse(readFileSync(path.join(webRoot, 'src', 'pi', pi, 'pi.config.json'), 'utf8'))
  const dest = path.resolve(webRoot, cfg.out)
  mkdirSync(path.dirname(dest), { recursive: true })
  copyFileSync(path.join(webRoot, 'dist', pi, 'index.html'), dest)

  console.log(`Built ${pi} -> ${dest}`)
}
