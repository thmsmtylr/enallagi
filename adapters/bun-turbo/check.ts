#!/usr/bin/env bun
import { $ } from 'bun'

const ROOT = import.meta.dir
const HASH_FILE = 'test-hashes.json'
const SCRIPTS_KEY = 'package.json#scripts'
const FOLD = 'packages/mcp/test/reference/fold.ts'
const XML = '.check/tests.xml'

// SPEC.md revision 2 §11: 18 rows in 11.1, 16 in 11.2, 12 in 11.3, 3 in 11.5, none in 11.4
const SPEC_ROW_COUNT = 49

// both ceilings are the maximum observed on f192298: measure/build.ts is 290 lines, measure/score.ts:89 is complexity 23
const MAX_LINES = 290
const MAX_COMPLEXITY = 23
const COMPLEXITY_RULE = `{"complexity":["error",${MAX_COMPLEXITY}]}`
const MAX_LINES_RULE = `{"max-lines":["error",{"max":${MAX_LINES}}]}`
// max-lines counts source only: a test file's length is set by the rows SPEC.md §11 assigns it
const TEST_GLOB = '**/*.test.ts'

type Row = { file: string; name: string }
type Hashes = Record<string, string>

const path = (relative: string) => `${ROOT}/${relative}`

const die = (message: string): never => {
  console.error(message)
  process.exit(1)
}

const sha256 = (bytes: ArrayBuffer) => new Bun.CryptoHasher('sha256').update(bytes).digest('hex')

const attr = (attrs: string, key: string) =>
  new RegExp(`\\b${key}="([^"]*)"`).exec(attrs)?.[1] ?? ''

async function specRows(): Promise<Row[]> {
  const spec = await Bun.file(path('SPEC.md')).text()
  const start = spec.indexOf('\n## 11. M1 exit criteria')
  if (start < 0) die('SPEC.md: no "## 11. M1 exit criteria" heading')
  const after = spec.indexOf('\n## 12.', start)
  const section = spec.slice(start, after < 0 ? undefined : after)
  const cells = section.matchAll(/^\|[^|]+\|\s*`([\w./-]+\.test\.ts)::([^`]+)`\s*\|/gm)
  return [...cells].map(m => ({ file: m[1] ?? '', name: m[2] ?? '' }))
}

async function rowsOrDie(): Promise<Row[]> {
  const rows = await specRows()
  if (rows.length !== SPEC_ROW_COUNT) {
    die(`SPEC.md §11 parsed to ${rows.length} rows, expected ${SPEC_ROW_COUNT} — a parser that finds fewer has found a table it cannot read`)
  }
  return rows
}

async function preloads(): Promise<string[]> {
  const file = Bun.file(path('bunfig.toml'))
  if (!(await file.exists())) return []
  const declared = /^\s*preload\s*=\s*(.+)$/m.exec(await file.text())?.[1] ?? ''
  return [...declared.matchAll(/"([^"]+)"/g)].map(m => m[1] ?? '')
}

async function immutableSet(rows: Row[]) {
  const spec = [...new Set(rows.map(r => `packages/mcp/${r.file}`))].sort()
  const optional = new Set([...spec, FOLD])
  const harness = ['bunfig.toml', 'check.ts', ...(await preloads())]
  return { all: [...spec, FOLD, ...harness], optional }
}

async function readHashes(): Promise<Hashes> {
  // the reference is the committed object, never the working tree: a tamper that edits a file and its digest must still fail
  const shown = await $`git show HEAD:${HASH_FILE}`.cwd(ROOT).quiet().nothrow()
  if (shown.exitCode !== 0) die(`no committed ${HASH_FILE} at HEAD (git exit ${shown.exitCode}) — precheck cannot verify anything, and a gate that verifies nothing is not a gate`)
  return JSON.parse(shown.text())
}

// every package.json the gate runs a script from, derived from root workspaces so an M2 package is covered unasked
async function scriptFiles(root = ROOT): Promise<string[]> {
  const globs: string[] = (await Bun.file(`${root}/package.json`).json()).workspaces ?? []
  const candidates = new Set(['package.json'])
  for (const pattern of globs) {
    for await (const found of new Bun.Glob(`${pattern}/package.json`).scan({ cwd: root })) candidates.add(found)
  }
  const out: string[] = []
  for (const relative of [...candidates].filter(f => !f.includes('node_modules/')).sort()) {
    const scripts = (await Bun.file(`${root}/${relative}`).json()).scripts ?? {}
    if (Object.keys(scripts).length) out.push(relative)
  }
  return out
}

async function scriptsHash(files: string[], root = ROOT): Promise<string> {
  const sorted: Record<string, Record<string, string>> = {}
  for (const relative of files) {
    const scripts: Record<string, string> = (await Bun.file(`${root}/${relative}`).json()).scripts ?? {}
    sorted[relative] = Object.fromEntries(Object.keys(scripts).sort().map(k => [k, scripts[k] ?? '']))
  }
  return sha256(new TextEncoder().encode(JSON.stringify(sorted)).buffer as ArrayBuffer)
}

async function scopeFiles(): Promise<string[]> {
  const found = await Array.fromAsync(new Bun.Glob('packages/mcp/**/*.ts').scan({ cwd: ROOT }))
  return found
    .filter(f => !f.includes('node_modules/'))
    .filter(f => !f.startsWith('packages/mcp/measure/') && !f.startsWith('packages/mcp/capture/'))
    .sort()
}

const FORBIDDEN = [
  { id: 'process.exit under src or test', pattern: /\bprocess\.exit\b/, where: 'scope' },
  { id: 'mock.module on a src module or on bun:test', pattern: /\bmock\.module\b/, where: 'scope' },
  { id: 'custom valueOf toJSON or equality override', pattern: /\b(valueOf|toJSON)\b|Symbol\.toPrimitive|^\s*equals\s*[(:]/, where: 'scope' },
  { id: 'branch on a fixture value inside src', pattern: /(?=.*(?:\bif\s*\(|\bswitch\s*\(|\bcase\s|\?|&&|\|\|))(?=.*(?:fixtures?\b|NODE_ENV|BUN_TEST))/i, where: 'product' },
  { id: 'only skip or todo in a criteria file', pattern: /\.(only|skip|todo)\s*\(/, where: 'criteria' },
] as const

async function forbidden(rows: Row[], problems: string[]) {
  const scope = await scopeFiles()
  const criteria = new Set(rows.map(r => `packages/mcp/${r.file}`))
  for (const file of scope) {
    const lines = (await Bun.file(path(file)).text()).split('\n')
    for (const rule of FORBIDDEN) {
      if (rule.where === 'product' && file.endsWith('.test.ts')) continue
      if (rule.where === 'criteria' && !criteria.has(file)) continue
      lines.forEach((line, i) => {
        if (rule.pattern.test(line)) problems.push(`§0.3 ${rule.id}: ${file}:${i + 1}  ${line.trim().slice(0, 80)}`)
      })
    }
  }
  console.log(`precheck: §0.3 read ${scope.length} files in packages/mcp outside measure/ and capture/`)
}

async function precheck() {
  const rows = await rowsOrDie()
  const { all, optional } = await immutableSet(rows)
  const hashes = await readHashes()
  const problems: string[] = []
  const pending: string[] = []
  let verified = 0
  for (const relative of all) {
    const file = Bun.file(path(relative))
    if (!(await file.exists())) {
      if (!optional.has(relative)) problems.push(`immutable file missing: ${relative}`)
      else pending.push(relative)
      continue
    }
    const want = hashes[relative]
    if (!want) {
      problems.push(`no hash entry for ${relative} — a file with no entry is a failure, not a skip`)
      continue
    }
    const got = sha256(await file.arrayBuffer())
    if (got !== want) problems.push(`hash mismatch for ${relative}: committed ${want.slice(0, 12)}, on disk ${got.slice(0, 12)}`)
    else verified += 1
  }
  const scripted = await scriptFiles()
  const wantScripts = hashes[SCRIPTS_KEY]
  const gotScripts = await scriptsHash(scripted)
  if (!wantScripts) problems.push(`no hash entry for ${SCRIPTS_KEY}`)
  else if (gotScripts !== wantScripts) problems.push(`hash mismatch for ${SCRIPTS_KEY} over ${scripted.join(' ')}: committed ${wantScripts.slice(0, 12)}, on disk ${gotScripts.slice(0, 12)}`)
  else verified += 1
  for (const key of Object.keys(hashes)) {
    if (key === SCRIPTS_KEY) continue
    if (!(await Bun.file(path(key)).exists())) problems.push(`${HASH_FILE} names a file that has vanished: ${key}`)
  }
  const note = pending.length ? `: ${pending.join(', ')}` : ''
  console.log(`precheck: verified ${verified} files including ${scripted.length} package.json script blocks (${scripted.join(' ')}), ${pending.length} §11 files not yet created${note}`)
  await forbidden(rows, problems)
  if (problems.length) die(`precheck: ${problems.length} failure(s):\n  ${problems.join('\n  ')}`)
}

function turbo(task: string, extra: string[], passthrough: string[]) {
  const args = ['turbo', task, ...extra, ...(passthrough.length ? ['--', ...passthrough] : [])]
  const { exitCode } = Bun.spawnSync(['bunx', ...args], { cwd: ROOT, stdio: ['inherit', 'inherit', 'inherit'] })
  if (exitCode !== 0) process.exit(exitCode ?? 1)
}

async function test(extra: string[]) {
  await $`mkdir -p ${path('.check')}`.quiet()
  await $`rm -f ${path(XML)}`.quiet()
  turbo('test', extra, [])
  if (!(await Bun.file(path(XML)).exists())) die(`test: ${XML} was not written — the suite did not run`)
}

async function trace() {
  const rows = await rowsOrDie()
  const file = Bun.file(path(XML))
  if (!(await file.exists())) die(`trace: ${XML} does not exist — run the test stage first`)
  const xml = await file.text()
  const seen = new Map<string, { assertions: number; isSkipped: boolean }>()
  for (const m of xml.matchAll(/<testcase\b([^>]*?)(\/>|>([\s\S]*?)<\/testcase>)/g)) {
    const attrs = m[1] ?? ''
    const body = m[3] ?? ''
    seen.set(`${attr(attrs, 'file')}::${attr(attrs, 'name')}`, {
      assertions: Number(attr(attrs, 'assertions') || '0'),
      isSkipped: /<skipped|<todo/.test(body),
    })
  }
  const problems: string[] = []
  for (const { file: f, name } of rows) {
    const found = seen.get(`${f}::${name}`)
    if (!found) problems.push(`no testcase ran for ${f}::${name}`)
    else if (found.isSkipped) problems.push(`skipped or todo: ${f}::${name}`)
    else if (found.assertions < 1) problems.push(`no assertion: ${f}::${name}`)
  }
  if (problems.length) die(`trace: ${rows.length} rows in SPEC.md §11, ${problems.length} unmet:\n  ${problems.join('\n  ')}`)
  console.log(`trace: all ${rows.length} rows in SPEC.md §11 map to a testcase that ran and asserted`)
}

async function selftest() {
  const root = path('.check/selftest')
  await $`rm -rf ${root}`.quiet()
  await Bun.write(`${root}/package.json`, JSON.stringify({ workspaces: ['packages/*'], scripts: { check: 'bun check.ts' } }))
  await Bun.write(`${root}/packages/fixture/package.json`, JSON.stringify({ scripts: { test: 'bun test --seed 1' } }))
  await Bun.write(`${root}/packages/scriptless/package.json`, JSON.stringify({ name: 'scriptless' }))
  const covered = await scriptFiles(root)
  await $`rm -rf ${root}`.quiet()
  const want = ['package.json', 'packages/fixture/package.json']
  if (covered.join(' ') !== want.join(' ')) die(`selftest: the scripts derivation covered [${covered.join(' ')}], expected [${want.join(' ')}]`)
  console.log(`selftest: the scripts derivation covered ${covered.length} of 3 fixture package.json files — workspace package in, scriptless package out`)
}

async function hash() {
  const rows = await rowsOrDie()
  const { all } = await immutableSet(rows)
  const present: string[] = []
  for (const relative of all) if (await Bun.file(path(relative)).exists()) present.push(relative)
  const scripted = await scriptFiles()
  const dirty = (await $`git status --porcelain -- ${[...present, ...scripted]}`.cwd(ROOT).text()).trim()
  if (dirty) die(`hash: uncommitted changes in the files it hashes — commit first:\n${dirty}`)
  const out: Hashes = {}
  for (const relative of present.sort()) out[relative] = sha256(await Bun.file(path(relative)).arrayBuffer())
  out[SCRIPTS_KEY] = await scriptsHash(scripted)
  await Bun.write(path(HASH_FILE), `${JSON.stringify(out, Object.keys(out).sort(), 2)}\n`)
  console.log(`hash: wrote ${Object.keys(out).length} entries to ${HASH_FILE}`)
}

const argv = process.argv.slice(2)
const stage = argv[0] && !argv[0].startsWith('-') ? argv[0] : ''
const extra = argv.filter(a => a.startsWith('-'))

const STAGES: Record<string, () => Promise<void>> = {
  precheck,
  selftest,
  typecheck: async () => turbo('typecheck', extra, []),
  lint: async () => {
    turbo('lint', extra, ['--rule', COMPLEXITY_RULE])
    turbo('lint', extra, ['--rule', MAX_LINES_RULE, '--ignore-pattern', TEST_GLOB])
  },
  test: () => test(extra),
  trace,
  hash,
}

if (stage) {
  const run = STAGES[stage]
  if (!run) die(`check: unknown stage ${stage} — one of ${Object.keys(STAGES).join(', ')}`)
  else await run()
} else {
  for (const name of ['precheck', 'selftest', 'typecheck', 'lint', 'test', 'trace']) {
    const run = STAGES[name]
    if (run) await run()
  }
}
