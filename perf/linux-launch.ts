// The launch of Brocode on a Linux desktop, in one command, for pasting back:
//
//   bun perf/linux-launch.ts [path to opencode-rune] [launches per case, 5]
//
// It starts the app on the session it is run from (Wayland or X11, the real compositor and GPU), in a
// home of its own under the temporary folder (nothing of the person's profile is read or written),
// and prints each stage in microseconds from the spawn call, the median of the launches:
//   first   a fresh home: the first launch ever (GPU shader cache and font cache of that home cold)
//   cold    a later launch with the exe's pages dropped from memory first: the first launch after a
//           boot or an update reads the exe from the disk
//   warm    a later launch, everything in memory
// The stages are the app's own launch marks (`--launch-trace`). With no OpenCode server at
// OPENCODE_LAUNCH_SERVER_SMALL the app has nothing to list, so `content.live` is left out and each
// launch ends by the app's own fallback about 12 s after it starts (the whole run: about 4 minutes).
// The first launch of a home downloads the browser engine in the background (350 MB) and keeps it for
// the later ones; OPENCODE_LAUNCH_SCRATCH moves the home.
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync } from "node:fs"
import { cpus, release, tmpdir } from "node:os"
import { join } from "node:path"

import { evict, warm } from "./linux-pages"

if (process.platform !== "linux") throw new Error("perf/linux-launch.ts runs on Linux")
const exe = process.argv[2] ?? Bun.which("opencode-rune") ?? ""
if (!exe || !existsSync(exe)) throw new Error("usage: bun perf/linux-launch.ts <path to opencode-rune> [launches per case]")
const N = Number(process.argv[3] ?? 5)
const SERVER = process.env.OPENCODE_LAUNCH_SERVER_SMALL ?? "http://127.0.0.1:5871"
const SCRATCH = process.env.OPENCODE_LAUNCH_SCRATCH ?? join(tmpdir(), "opencode-rune-launch-bench", "linux")
const home = join(SCRATCH, "home")
const engine = join(home, "state", "co.anomaly.opencode", "browser-engine")
const served = await fetch(`${SERVER}/api/info`, { signal: AbortSignal.timeout(1000) }).then((r) => r.ok).catch(() => false)

/** The launch's own home (every folder the app reads or writes) on the session this was started from. */
function seal() {
  for (const d of ["home/Documents", "tmp", "xdg/config", "xdg/state", "xdg/data", "xdg/cache"]) mkdirSync(join(home, d), { recursive: true })
  const session = ["WAYLAND_DISPLAY", "DISPLAY", "XAUTHORITY", "XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE", "XDG_SESSION_DESKTOP", "DESKTOP_SESSION", "LANG"]
  return {
    ...Object.fromEntries(session.flatMap((k) => (process.env[k] ? [[k, process.env[k]]] : []))),
    PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
    RUNE_DIRS_ROOT: home, OPENCODE_RUNE_HOME: home, HOME: join(home, "home"), TMPDIR: join(home, "tmp"),
    XDG_CONFIG_HOME: join(home, "xdg", "config"), XDG_STATE_HOME: join(home, "xdg", "state"), XDG_DATA_HOME: join(home, "xdg", "data"), XDG_CACHE_HOME: join(home, "xdg", "cache"),
    OPENCODE_SERVER_PASSWORD: "parity-harness", OPENCODE_RUNE_OTEL: "0",
  } as Record<string, string>
}

type Launch = { marks: Record<string, number>; notes: Record<string, string>; renderer: string }

/** One launch; its marks in microseconds from the spawn call. */
async function launch(): Promise<Launch | undefined> {
  const trace = join(home, "launch-trace.jsonl")
  rmSync(trace, { force: true })
  const spawned = (performance.timeOrigin + performance.now()) * 1000
  const p = Bun.spawn([exe, "--launch-marks", "--launch-trace", trace, "--launch-trace-quit", "300", "--server", served ? SERVER : "http://127.0.0.1:9"], { env: seal(), stdout: "pipe", stderr: "ignore", stdin: "ignore" })
  const timer = setTimeout(() => p.kill(), 45_000)
  const out = await new Response(p.stdout).text()
  await p.exited
  clearTimeout(timer)
  // `main.entered`'s own wall-clock time, to the microsecond; the trace's other marks count from it.
  const main = /launch-mark main\.entered \d+ at=(\d+\.\d+)/.exec(out)
  if (!main || !existsSync(trace)) return undefined
  const line = JSON.parse(readFileSync(trace, "utf8").trim().split("\n").at(-1)!)
  const from = Number(main[1]) * 1000 - spawned - line.marks_ns["main.entered"] / 1000
  return { marks: Object.fromEntries(Object.entries(line.marks_ns as Record<string, number>).map(([k, ns]) => [k, Math.round(from + ns / 1000)])), notes: line.notes ?? {}, renderer: line.renderer }
}

const STAGES = ["main.entered", "loop.built", "window.create", "window.native", "gpu.adapter", "gpu.device", "renderer.ready", "window.created", "frame.first.build", "frame.first.encoded", "frame.first.presented", "input.ready", ...(served ? ["server.answered", "content.live"] : [])]
const median = (v: number[]) => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)]!

async function run(name: string, before: () => void) {
  const launches: Launch[] = []
  for (let i = 0; i < N; i++) {
    before()
    const one = await launch()
    if (one) launches.push(one)
    process.stderr.write(`${name} ${i + 1}/${N}${one ? ` first frame ${one.marks["frame.first.presented"]} us` : " did not reach its first frame"}\n`)
  }
  const stages = Object.fromEntries(STAGES.flatMap((s) => {
    const v = launches.flatMap((l) => (l.marks[s] === undefined ? [] : [l.marks[s]!]))
    return v.length ? [[s, { median: median(v), min: Math.min(...v), max: Math.max(...v) }]] : []
  }))
  return { case: name, launches: launches.length, stages, notes: launches.at(-1)?.notes ?? {}, renderer: launches.at(-1)?.renderer }
}

/** A fresh home that keeps an installed browser engine, so each first launch does not download it again. */
function fresh() {
  const kept = join(SCRATCH, "engine-kept")
  if (existsSync(engine)) renameSync(engine, kept)
  rmSync(home, { recursive: true, force: true })
  if (existsSync(kept)) {
    mkdirSync(join(engine, ".."), { recursive: true })
    renameSync(kept, engine)
  }
}

const results = [await run("first", fresh)]
// The profile a later launch starts from: two untimed launches.
await launch()
await launch()
results.push(await run("cold", () => evict(exe)))
results.push(await run("warm", () => warm(exe)))

const read = (path: string) => (existsSync(path) ? readFileSync(path, "utf8") : "")
const machine = {
  app: Bun.spawnSync([exe, "--version"]).stdout.toString().trim(),
  exe_bytes: statSync(exe).size,
  kernel: release(),
  distro: /PRETTY_NAME="?([^"\n]*)/.exec(read("/etc/os-release"))?.[1],
  cpu: `${cpus()[0]?.model} x${cpus().length}`,
  session: process.env.XDG_SESSION_TYPE,
  desktop: process.env.XDG_CURRENT_DESKTOP,
  wayland: Boolean(process.env.WAYLAND_DISPLAY),
  server: served,
}
console.log(`\n${machine.app} on ${machine.distro}, ${machine.kernel}, ${machine.cpu}; ${machine.session ?? "?"} session, ${machine.desktop ?? "?"}; GPU ${results.at(-1)?.notes.adapter ?? "?"}`)
console.log(`microseconds from the spawn call, median of ${N} (min-max)\n`)
console.log(["stage".padEnd(24), ...results.map((r) => r.case.padStart(30))].join(""))
for (const s of STAGES) console.log([s.padEnd(24), ...results.map((r) => (r.stages[s] ? `${r.stages[s].median} (${r.stages[s].min}-${r.stages[s].max})` : "-").padStart(30))].join(""))
console.log(`\nlinux-launch ${JSON.stringify({ machine, results })}`)
