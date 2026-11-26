// From the window to content: a later launch of Brocode that restores what the window last
// showed, against a server that holds a real-size history, timed to what a person is waiting for:
// the open session's last messages from the saved copy and then from the server, and the last
// change of the window's pixels.
//
//   strobe bench perf/content.bench.ts --head exe:<opencode-desktop.exe> --check           each case once
//   strobe bench perf/content.bench.ts --base exe:<old> --head exe:<new>                   did content come sooner?
//   strobe record --bench perf/content.bench.ts#live --head exe:<build with a PDB>            stacks, file:line
//   strobe query <rec> hotspots --thread main --sort total
//
// The server is not started here: it is the one a person has. Name it in the environment:
//
//   OPENCODE_BENCH_SERVER    its address (http://127.0.0.1:<port>); never 4096, the app's own default
//   OPENCODE_BENCH_PASSWORD  its password
//   OPENCODE_BENCH_SESSION   the session the window had open (its id)
//
// The app is given no `--server`: each launch finds the server as an installed copy does, from the
// registration `<state>/opencode/service.json`, which `before` writes into the launch's own home.
// The home is a copy of a profile the build under test made itself (one launch that opens the
// session, one that settles, both quit as a person quits), so every timed launch is that build's
// next launch after a clean quit.
import { bench, native } from "@hona/strobe"
import { createHash } from "node:crypto"
import { cpSync, existsSync, mkdirSync, readdirSync, rmSync, statSync, symlinkSync, writeFileSync } from "node:fs"
import { createConnection } from "node:net"
import { tmpdir } from "node:os"
import { basename, dirname, join } from "node:path"

const SERVER = process.env.OPENCODE_BENCH_SERVER ?? ""
const PASSWORD = process.env.OPENCODE_BENCH_PASSWORD ?? ""
const SESSION = process.env.OPENCODE_BENCH_SESSION ?? ""
const CACHE = process.env.OPENCODE_BENCH_CACHE ?? join(tmpdir(), "opencode-rune-bench")
// An installed browser engine (`<state>/co.anomaly.opencode/browser-engine/<version>` of a profile that has one):
// linked into every launch's home, as a later launch has it. Without it each launch starts the engine's download.
const ENGINE = process.env.OPENCODE_BENCH_ENGINE ?? ""

// Each milestone is a line the app prints with the mark's own time (`--launch-marks`).
const mark = (name: string) => ({ match: new RegExp(`launch-mark ${name.replace(/\./g, "\\.")} \\d+ at=(?<at>\\d{13}(?:\\.\\d+)?)`), in: "output" as const })
const milestones = {
  first_frame: mark("frame.first.presented"),
  input_ready: mark("input.ready"),
  // The first frame with the open session's messages: the saved copy's, on a later launch.
  rows: mark("content.rows"),
  // The first frame with sessions to list (Home's rows, the sidebar's).
  list: mark("sessions.rows"),
  server: mark("server.answered"),
  // The server answered and the window shows its page.
  content_live: mark("content.live"),
  // The open session's newest page came from the server: its rows are the server's from here.
  session_read: mark("session.read.received"),
  pixels_stable: mark("pixels.stable"),
}
// `content.rows` and `session.read.received` are printed by builds from 24 Nov 2026: OPENCODE_BENCH_ROWS=0 for an older one.
const rows = process.env.OPENCODE_BENCH_ROWS === "0" ? [] : ["launch.rows", "launch.session_read"]
const marks = ["launch.first_frame", "launch.input_ready", ...rows, "launch.server", "launch.content_live"]
const outside = ["launch.painted", "cpu.work", "memory.app.private"]
type Hook = { home: string; env: Record<string, string>; exe: string }

const listening = (port: number) =>
  new Promise<boolean>((resolve) => {
    const socket = createConnection({ host: "127.0.0.1", port })
    const done = (yes: boolean) => (socket.destroy(), resolve(yes))
    socket.setTimeout(400, () => done(false))
    socket.once("connect", () => done(true))
    socket.once("error", () => done(false))
  })

/** The server's `/api/info`, or why the bench can't run against it. */
async function info() {
  if (!/^http:\/\/127\.0\.0\.1:\d+$/.test(SERVER) || SERVER.endsWith(":4096")) throw new Error("set OPENCODE_BENCH_SERVER to a server of the bench's own, http://127.0.0.1:<port> (not 4096)")
  const r = await fetch(`${SERVER}/api/info`, { headers: { authorization: "Basic " + Buffer.from(`opencode:${PASSWORD}`).toString("base64") }, signal: AbortSignal.timeout(3000) }).catch(() => undefined)
  if (!r?.ok) throw new Error(`${SERVER} did not answer /api/info (${r?.status ?? "no connection"})`)
  // With the bench's server down the app would try its default address next: whatever listens
  // there is the person's own.
  if (await listening(4096)) throw new Error("something listens on 127.0.0.1:4096, the app's default address: a launch whose server went away would reach it")
  return (await r.json()) as { version: string; pid: number }
}

async function register(home: string) {
  const { version, pid } = await info()
  const dir = join(home, "xdg", "state", "opencode")
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, "service.json"), JSON.stringify({ id: "opencode-rune-bench", version, url: SERVER, pid, password: PASSWORD }))
}

/** Windows runs a plugin's `opencode`, `ssh` and `wsl` from the app's own folder first: a bench exe has stand-ins there. */
function sealed(exe: string) {
  if (process.platform !== "win32") return
  for (const name of ["opencode.exe", "ssh.exe", "wsl.exe"]) if (!existsSync(join(dirname(exe), name))) throw new Error(`${dirname(exe)} has no stand-in ${name}: the app's plugins would run this machine's own`)
}

/** One launch of `exe` in `home` that quits by itself once its content has been up for `ms`. */
function settle(hook: Hook, home: string, args: string[], ms: number) {
  const env = Object.fromEntries(Object.entries(hook.env).map(([k, v]) => [k, v.split(hook.home).join(home).split(hook.home.replaceAll("\\", "/")).join(home.replaceAll("\\", "/"))]))
  const r = Bun.spawnSync([hook.exe, ...args, "--launch-trace", join(home, "setup-trace.jsonl"), "--launch-trace-quit", String(ms)], { env, cwd: dirname(hook.exe), stdout: "ignore", stderr: "ignore", timeout: 60_000 })
  if (r.exitCode !== 0) throw new Error(`the setup launch of ${hook.exe} ended with ${r.exitCode}`)
}

/** The profile a later launch starts from, made once per exe and state by the exe itself. */
async function template(hook: Hook, state: string, shape: (home: string) => void) {
  const stat = statSync(hook.exe)
  const key = createHash("sha256").update(`${hook.exe}|${stat.size}|${stat.mtimeMs}|${SERVER}|${SESSION}|${state}`).digest("hex").slice(0, 16)
  const dir = join(CACHE, "profiles", `${basename(dirname(hook.exe))}-${state}-${key}`)
  if (existsSync(join(dir, "ready"))) return dir
  rmSync(dir, { recursive: true, force: true })
  for (const d of ["home/AppData/Roaming", "home/AppData/Local", "home/Documents", "tmp", "xdg/config", "xdg/data", "xdg/cache"]) mkdirSync(join(dir, d), { recursive: true })
  await register(dir)
  settle(hook, dir, ["--session", SESSION], 3000)
  shape(dir)
  settle(hook, dir, [], 3000)
  for (const f of readdirSync(dir)) if (f.endsWith(".jsonl") || f.endsWith(".log")) rmSync(join(dir, f), { force: true })
  writeFileSync(join(dir, "ready"), new Date().toISOString())
  return dir
}

/** Before every launch: the home is the template's copy, registered with the server. */
const restored = (state: string, shape: (home: string) => void = () => {}) => async (ctx: Hook) => {
  if (!SESSION) throw new Error("set OPENCODE_BENCH_SESSION to the session the window had open")
  sealed(ctx.exe)
  cpSync(await template(ctx, state, shape), ctx.home, { recursive: true })
  rmSync(join(ctx.home, "ready"), { force: true })
  const engines = join(ctx.home, "state", "co.anomaly.opencode", "browser-engine")
  rmSync(engines, { recursive: true, force: true })
  if (ENGINE) {
    mkdirSync(engines, { recursive: true })
    symlinkSync(ENGINE, join(engines, basename(ENGINE)), "junction")
  }
  await register(ctx.home)
}

export default bench({
  targets: {
    opencode: native("opencode-rune", { name: "Brocode, a real server", output: true, viewport: { width: 1280, height: 800 } }),
  },
  cases: {
    // To the app's own "content live": the server answered and the open session's rows show.
    live: {
      primary: "launch.content_live",
      hero: true,
      metrics: [...marks, ...outside],
      launch: { until: { milestone: "content_live" }, milestones, profile: "fresh", before: restored("session") },
    },
    // To the open session's newest page from the server. Not `until` a row of the timeline: the saved copy
    // puts the session's last turns in the first frame, so strobe finds them in its first read of the window.
    session: {
      primary: "launch.session_read",
      metrics: [...marks, ...outside],
      launch: { until: { milestone: "session_read" }, milestones, profile: "fresh", before: restored("session") },
    },
    // The window's last frame of change: everything it restores is on screen and holds still. Needs the
    // window's pixels (a desktop a person could see; refused `NoPresentSource` on a hidden one).
    settled: {
      primary: "launch.ready",
      metrics: [...marks, "launch.pixels_stable", "launch.settled", ...outside],
      launch: { until: { window: "settled" }, milestones, profile: "fresh", before: restored("session") },
    },
  },
})
