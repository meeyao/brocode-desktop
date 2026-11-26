// One launch of Brocode on macOS as the Dock and Finder start it (Launch Services: `open`), for
// `strobe run`. strobe cannot spawn the app itself there (launchd does), so the numbers are the app's own launch
// marks (`--launch-trace`, µs from the kernel's start time of the process), printed as `strobe:metric` lines.
//
//   strobe run --base "bun perf/macos-open.ts <old .app>" --head "bun perf/macos-open.ts <new .app>" \
//     --primary metric.first_frame --metrics metric.input_ready,metric.content_live,metric.main,metric.resumed,metric.window
//
//   bun perf/macos-open.ts <.app> [small|first|cold] [child]
//     small  a later launch: the profile this build made (two untimed launches, kept in OPENCODE_LAUNCH_SCRATCH)
//     first  no profile: the first launch ever
//     cold   small, from a copy of the bundle at a new path: Launch Services has never seen it and its code
//            signature is checked again (the copy shares the original's blocks, so its pages may be in memory)
//     child  the bundle's exe started as a child process instead of through `open`
// A path with no spaces, or quote it for strobe's command line. The fixture server must already answer at
// OPENCODE_LAUNCH_SERVER_SMALL (default http://127.0.0.1:5871): a run that left one behind would be refused.
// OPENCODE_LAUNCH_PATH is the PATH the app gets (default /usr/bin:/bin:/usr/sbin:/sbin: no OpenCode CLI on it).
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { homedir, tmpdir } from "node:os"
import { join } from "node:path"

const [bundleArg, profile = "small", how = "open"] = process.argv.slice(2)
if (!bundleArg) throw new Error("usage: bun perf/macos-open.ts <.app> [small|first|cold] [child]")
const SERVER = process.env.OPENCODE_LAUNCH_SERVER_SMALL ?? "http://127.0.0.1:5871"
const SESSION = process.env.OPENCODE_LAUNCH_SESSION ?? "ses_e37949d95ffeScaleMain00000"
const SCRATCH = process.env.OPENCODE_LAUNCH_SCRATCH ?? join(tmpdir(), "opencode-rune-launch-bench")
mkdirSync(SCRATCH, { recursive: true })

if (!(await fetch(`${SERVER}/api/info`, { signal: AbortSignal.timeout(1000) }).then((r) => r.ok).catch(() => false))) throw new Error(`no OpenCode server answers at ${SERVER}: start the fixture first (perf/README.md)`)

/** The launch's own home: every folder the app reads or writes, a mock Keychain, no telemetry. */
function seal(root: string) {
  for (const d of ["home/Documents", "tmp", "xdg/config/opencode", "xdg/state", "xdg/data", "xdg/cache"]) mkdirSync(join(root, d), { recursive: true })
  return {
    RUNE_DIRS_ROOT: root, OPENCODE_RUNE_HOME: root, HOME: join(root, "home"), OPENCODE_SERVER_PASSWORD: "parity-harness", OPENCODE_RUNE_OTEL: "0",
    TMPDIR: join(root, "tmp"), XDG_CONFIG_HOME: join(root, "xdg", "config"), XDG_STATE_HOME: join(root, "xdg", "state"), XDG_DATA_HOME: join(root, "xdg", "data"), XDG_CACHE_HOME: join(root, "xdg", "cache"),
    OPENCODE_BROWSER_MOCK_KEYCHAIN: "1", OPENCODE_CONFIG_DIR: join(root, "xdg", "config", "opencode"),
    PATH: process.env.OPENCODE_LAUNCH_PATH ?? "/usr/bin:/bin:/usr/sbin:/sbin",
    // Arms of one build: `RUNE_WINDOW_AFTER_LAUNCH=1 bun perf/macos-open.ts …` reaches the app.
    ...Object.fromEntries(Object.entries(process.env).filter(([k]) => k.startsWith("RUNE_WINDOW_") || k.startsWith("MOTION_STANDIN_"))),
  } as Record<string, string>
}

async function launch(bundle: string, root: string, extra: string[], quit: number, way: string) {
  const env = seal(root)
  const trace = join(root, "launch-trace.jsonl")
  rmSync(trace, { force: true })
  const args = ["--server", SERVER, "--launch-trace", trace, "--launch-trace-quit", String(quit), ...extra]
  const cmd = way === "child"
    ? [join(bundle, "Contents", "MacOS", "opencode-desktop"), ...args]
    : ["open", "-n", "-W", "-a", bundle, ...Object.entries(env).flatMap(([k, v]) => ["--env", `${k}=${v}`]), "--args", ...args]
  const p = Bun.spawn(cmd, { env: way === "child" ? env : { PATH: "/usr/bin:/bin", HOME: homedir() }, stdout: "ignore", stderr: "ignore", stdin: "ignore" })
  const timer = setTimeout(() => p.kill(9), 30_000)
  await p.exited
  clearTimeout(timer)
  if (!existsSync(trace)) throw new Error(`no launch trace from ${bundle}`)
  return JSON.parse(readFileSync(trace, "utf8").trim().split("\n").pop()!).marks_ns as Record<string, number>
}

const tpl = join(SCRATCH, "open-" + Bun.hash(bundleArg + ":" + Bun.file(join(bundleArg, "Contents", "MacOS", "opencode-desktop")).size).toString(16))
if (profile !== "first" && !existsSync(join(tpl, "ready"))) {
  rmSync(tpl, { recursive: true, force: true })
  mkdirSync(tpl, { recursive: true })
  await launch(bundleArg, tpl, ["--session", SESSION], 4000, "child")
  await launch(bundleArg, tpl, [], 4000, "child")
  rmSync(join(tpl, "launch-trace.jsonl"), { force: true })
  writeFileSync(join(tpl, "ready"), new Date().toISOString())
}

const root = mkdtempSync(join(SCRATCH, "run-"))
try {
  if (profile !== "first") cpSync(tpl, root, { recursive: true })
  const bundle = profile === "cold" ? join(root, "copy", "OpenCode Rune.app") : bundleArg
  if (profile === "cold") {
    mkdirSync(join(root, "copy"), { recursive: true })
    Bun.spawnSync(["cp", "-R", bundleArg, bundle])
  }
  const m = await launch(bundle, root, [], 1200, how)
  if (!m["frame.first.presented"]) throw new Error("no first frame in the trace")
  const marks = { first_frame: "frame.first.presented", main: "main.entered", loop_built: "loop.built", resumed: "loop.resumed", window: "window.created", gpu_done: "frame.first.gpu_done", input_ready: "input.ready", content_live: "content.live" }
  for (const [name, mark] of Object.entries(marks)) if (m[mark]) console.log(`strobe:metric ${name}=${Math.round(m[mark]! / 1000)}`)
} finally {
  rmSync(root, { recursive: true, force: true })
}
