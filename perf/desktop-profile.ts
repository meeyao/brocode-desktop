// What OpenCode Desktop (the official app) leaves in its `userData` folder, made up: a
// `drafts.sqlite` with a window's tabs, drafts and attachment blobs up to a size, `opencode.settings`
// and the window's frame. A new profile of Brocode seeds itself from it on its first launch
// (`crates/opencode-storage/src/native/seed`), which `launch.bench.ts` times.
//
//   bun perf/desktop-profile.ts <folder> [--mb 64] [--running]
//
// `--running`: as while Desktop runs: a WAL with frames beside the database.
import { Database } from "bun:sqlite"
import { createHash } from "node:crypto"
import { cpSync, mkdirSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs"
import { join } from "node:path"

export type Desktop = { mb: number; running: boolean; session: string }

/** Desktop's own migrations (`packages/desktop/src/main/storage/migration.ts`), in order. */
const MIGRATIONS: [string, string[]][] = [
  ["20260906234153_drafts", ["CREATE TABLE `blob` (`id` text PRIMARY KEY, `data` blob NOT NULL);", "CREATE TABLE `document` (`key` text PRIMARY KEY, `value` text NOT NULL);"]],
  ["20260906234220_state", ["CREATE TABLE `state` (`name` text NOT NULL, `key` text NOT NULL, `value` text NOT NULL, `updated_at` integer NOT NULL, CONSTRAINT `state_pk` PRIMARY KEY(`name`, `key`));"]],
  ["20260907031611_blob-touched", ["ALTER TABLE `blob` ADD `touched_at` integer DEFAULT 0 NOT NULL;"]],
  ["20260930041551_extension", ["CREATE TABLE `plugin` (`id` text PRIMARY KEY, `enabled` integer DEFAULT true NOT NULL, `manifest` text, `revision` text);"]],
]

/** Writes the folder at `dir`: one window (`main`) with six session tabs, the first of them `session`. */
export function desktopProfile(dir: string, opts: Desktop) {
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, "opencode.settings"), JSON.stringify({ windowIds: ["main"], firstLaunchOnboardingComplete: true, zoomLevel: 1, backgroundColor: "#121212" }, null, "\t"))
  writeFileSync(join(dir, "window-state-main.json"), JSON.stringify({ x: 120, y: 80, width: 1280, height: 800, isMaximized: false, isFullScreen: false, displayBounds: { x: 0, y: 0, width: 1920, height: 1200 } }))
  const tabs = Array.from({ length: 6 }, (_, i) => ({ type: "session", server: "sidecar", sessionID: i === 0 ? opts.session : `ses_e37949d95ffeScaleSes${String(i).padStart(6, "0")}` }))
  const info = Object.fromEntries(tabs.map((t, i) => [`session:sidecar:${t.sessionID}`, { title: `Session ${i}`, directory: "/repo" }]))
  const b64 = Buffer.from("sidecar").toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "")
  const file = join(dir, "drafts.sqlite")
  const db = new Database(file)
  db.exec("PRAGMA page_size = 16384; PRAGMA journal_mode = WAL; CREATE TABLE migration (id TEXT PRIMARY KEY, time_completed INTEGER NOT NULL);")
  const now = Date.now()
  for (const [id, statements] of MIGRATIONS) {
    for (const s of statements) db.exec(s)
    db.query("INSERT INTO migration (id, time_completed) VALUES (?1, ?2)").run(id, now)
  }
  const state = db.query("INSERT INTO state (name, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)")
  db.transaction(() => {
    state.run("opencode.global.dat", "language", JSON.stringify({ locale: "en" }), now)
    state.run("opencode.window.main.dat", "tabs", JSON.stringify(tabs), now)
    state.run("opencode.window.main.dat", "tabs.info", JSON.stringify(info), now)
    state.run("opencode.window.main.dat", "lastActiveUrl", JSON.stringify(`/server/${b64}/session/${opts.session}`), now)
  })()
  // Attachments of drafts long sent: what makes a real file big, and nothing the seed takes.
  const blob = db.query("INSERT INTO blob (id, data, touched_at) VALUES (?1, ?2, ?3)")
  const chunk = Buffer.alloc(256 * 1024, 0x42)
  for (let i = 0; statSync(file).size < opts.mb * 1024 * 1024; ) {
    db.transaction(() => {
      for (let n = 0; n < 32; n++) {
        chunk.writeUInt32LE(i++, 0)
        blob.run(createHash("sha256").update(chunk.subarray(0, 64)).digest("hex"), chunk, now)
      }
    })()
    db.exec("PRAGMA wal_checkpoint(TRUNCATE)")
  }
  if (!opts.running) return db.close()
  // A row written and not checkpointed, the files copied while the connection that wrote it is open.
  db.exec("PRAGMA wal_autocheckpoint = 0")
  db.query("UPDATE state SET updated_at = ?1 WHERE key = 'tabs'").run(now + 1)
  const live = `${dir}.live`
  rmSync(live, { recursive: true, force: true })
  mkdirSync(live, { recursive: true })
  for (const f of readdirSync(dir)) if (!f.endsWith("-shm")) cpSync(join(dir, f), join(live, f))
  db.close()
  rmSync(dir, { recursive: true, force: true })
  renameSync(live, dir)
}

if (import.meta.main) {
  const argv = process.argv.slice(2)
  if (!argv[0]) {
    console.error("usage: bun perf/desktop-profile.ts <folder> [--mb 64] [--running]")
    process.exit(2)
  }
  desktopProfile(argv[0], { mb: Number(argv.includes("--mb") ? argv[argv.indexOf("--mb") + 1] : 64), running: argv.includes("--running"), session: "ses_e37949d95ffeScaleMain00000" })
}
