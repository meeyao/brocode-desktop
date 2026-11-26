// The launch of Brocode, measured by strobe from the process's spawn: the four launches a
// person gets, against three sizes of profile. README.md beside this file says how to run it and
// how to get a build with symbols.
//
//   strobe bench perf/launch.bench.ts --base exe:<old>/opencode-desktop.exe --head exe:<new>/opencode-desktop.exe --filter warm --target small
//   strobe bench perf/launch.bench.ts --head exe:<exe> --check                         # every case once, no verdict
//   strobe record --bench perf/launch.bench.ts#cold --head exe:<exe> --target small    # stacks: then `strobe query <rec> hotspots`
//
// Cases (the launch):
//   first            a fresh profile, never run before (target `none` only)
//   first-desktop    a fresh profile on a machine where OpenCode Desktop (the official app) left a database:
//                    the first launch seeds its windows, tabs and drafts from it (target `none` only)
//   first-desktop-running   the same while Desktop runs: its database has a WAL with frames
//   first-new-build  a profile an earlier run made, a new exe file, none of the exe's own caches: the launch after an update
//   cold             the exe's pages not in memory (a new file with the same bytes), GPU shader cache cold
//   warm             a later launch, everything cached
// Targets (the profile, and the server behind it):
//   none   no profile; small   5 projects, 12 sessions, a 10-message session open;
//   big    40 projects, 4,800 sessions, a 10,000-message session open
// Milestones are the app's own `--launch-trace` marks, printed by `--launch-marks` with the mark's own time:
//   launch.main (`main.entered`), launch.window (`window.created`), launch.first_frame
//   (`frame.first.presented`), launch.input_ready, launch.server (`server.answered`), launch.content_live.
import { dlopen, FFIType, ptr, toArrayBuffer } from "bun:ffi"
import { closeSync, cpSync, existsSync, mkdirSync, openSync, readSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { basename, dirname, join } from "node:path"
import { bench, native } from "@hona/strobe"

import { desktopProfile } from "./desktop-profile"
import { evict, warm } from "./linux-pages"

// The OpenCode server the app talks to: a fixture that answers the v2 API with a profile's worth of
// data. It is not in this repository yet. Give running servers (OPENCODE_LAUNCH_SERVER_SMALL, _BIG:
// their URLs) or a script this file starts once per size and leaves to its own exit
// (OPENCODE_LAUNCH_FIXTURE: `bun <script> --port <p> --projects <n> --sessions <n> --messages <n>`).
const SHAPES = {
  none: { port: Number(process.env.OPENCODE_LAUNCH_PORT_SMALL ?? 5871), projects: 5, sessions: 12, messages: 10 },
  small: { port: Number(process.env.OPENCODE_LAUNCH_PORT_SMALL ?? 5871), projects: 5, sessions: 12, messages: 10 },
  big: { port: Number(process.env.OPENCODE_LAUNCH_PORT_BIG ?? 5872), projects: 40, sessions: 4800, messages: 10000 },
} as const
type Shape = keyof typeof SHAPES
const serverUrl = (shape: Shape) => process.env[shape === "big" ? "OPENCODE_LAUNCH_SERVER_BIG" : "OPENCODE_LAUNCH_SERVER_SMALL"] ?? `http://127.0.0.1:${SHAPES[shape].port}`
// The session the saved window of `small` and `big` has open (the fixture's long session).
const SESSION = process.env.OPENCODE_LAUNCH_SESSION ?? "ses_e37949d95ffeScaleMain00000"
// Profiles made once per exe and size, and copied into each launch's home.
const SCRATCH = process.env.OPENCODE_LAUNCH_SCRATCH ?? join(tmpdir(), "opencode-rune-launch-bench")

async function up(shape: Shape) {
  return await fetch(`${serverUrl(shape)}/api/info`, { signal: AbortSignal.timeout(500) }).then((r) => r.ok).catch(() => false)
}

async function server(shape: Shape) {
  if (await up(shape)) return
  const script = process.env.OPENCODE_LAUNCH_FIXTURE
  if (!script) throw new Error(`no OpenCode server answers at ${serverUrl(shape)}: start one, or set OPENCODE_LAUNCH_FIXTURE to the fixture server's script (perf/README.md)`)
  const s = SHAPES[shape]
  Bun.spawn(["bun", script, "--port", String(s.port), "--projects", String(s.projects), "--sessions", String(s.sessions), "--messages", String(s.messages)], { stdout: "ignore", stderr: "ignore", stdin: "ignore", windowsHide: true }).unref()
  for (let i = 0; i < 150 && !(await up(shape)); i++) await Bun.sleep(100)
  if (!(await up(shape))) throw new Error(`the fixture server for ${shape} did not answer at ${serverUrl(shape)}`)
  // Its lists built before a timed launch reads them.
  for (let i = 0; i < 3; i++) await fetch(`${serverUrl(shape)}/api/session?limit=120`).then((r) => r.arrayBuffer()).catch(() => {})
}

type Before = { case: string; target: string; home: string | null; env: Record<string, string> | null; exe: string | null }

/** The folders the profile's environment names, which the app expects to exist. */
function folders(home: string) {
  for (const d of ["home/AppData/Local", "home/AppData/Roaming", "home/Documents", "tmp"]) mkdirSync(join(home, d), { recursive: true })
}

/** One launch of `exe` outside strobe's clock, in `home`, quitting by itself once its content is live. */
async function settle(exe: string, env: Record<string, string>, from: string, home: string, shape: Shape, extra: string[]) {
  folders(home)
  const moved = Object.fromEntries(Object.entries(env).map(([k, v]) => [k, v.split(from).join(home).split(from.replaceAll("\\", "/")).join(home.replaceAll("\\", "/"))]))
  const p = Bun.spawn([exe, "--server", serverUrl(shape), "--launch-trace", join(home, "launch-trace.jsonl"), "--launch-trace-quit", "4000", ...extra], { env: moved, cwd: dirname(exe), stdout: "ignore", stderr: "ignore", stdin: "ignore" })
  const timer = setTimeout(() => p.kill(), 60_000)
  await p.exited
  clearTimeout(timer)
  if (!existsSync(join(home, "launch-trace.jsonl"))) throw new Error(`${exe} did not finish a launch in ${home} (no launch trace): does it start by hand with --server ${serverUrl(shape)}?`)
  rmSync(join(home, "launch-trace.jsonl"), { force: true })
}

/** The profile a later launch starts from: made by `exe` itself (its own schema), one launch that
 *  opens the session and one that settles its caches. Kept per exe (path, size, time) and size. */
async function template(exe: string, env: Record<string, string>, from: string, shape: Shape) {
  const st = statSync(exe)
  const dir = join(SCRATCH, "profiles", `${basename(dirname(exe))}-${st.size}-${Math.round(st.mtimeMs)}-${shape}-noengine`)
  if (existsSync(join(dir, "ready"))) return dir
  rmSync(dir, { recursive: true, force: true })
  mkdirSync(dir, { recursive: true })
  await settle(exe, env, from, dir, shape, ["--session", SESSION])
  await settle(exe, env, from, dir, shape, [])
  // The browser engine downloads in the background while those two launches run, and how far it got
  // is chance: one build's template held 32 MB of it and the other's 82 MB, copied into every launch's
  // home right before the timed launch. A template holds none of it.
  rmSync(join(dir, "state", "co.anomaly.opencode", "browser-engine"), { recursive: true, force: true })
  writeFileSync(join(dir, "ready"), new Date().toISOString())
  return dir
}

/** The files that hold the app's code: the exe, and beside a packaged Windows build's thin exe the library it
 *  loads the app from (the exe's name with `.dll`; apps/desktop-thin). Cold and warm are about both. */
function images(exe: string) {
  const library = exe.replace(/\.exe$/i, ".dll")
  return process.platform === "win32" && library !== exe && existsSync(library) ? [exe, library] : [exe]
}

/** The exe as a new file with the same bytes, at the same path: nothing of it is in memory, and
 *  what Windows keeps per file (its mapping, a scanner's verdict) starts again. The copy is
 *  unbuffered, so the file cache holds none of it either. */
function coldExe(exe: string) {
  // robocopy copies a file under its own name only: to a folder beside it, then moved over the old one.
  const aside = join(dirname(exe), ".cold")
  rmSync(aside, { recursive: true, force: true })
  mkdirSync(aside, { recursive: true })
  if (process.platform === "win32") Bun.spawnSync(["robocopy", dirname(exe), aside, basename(exe), "/J", "/NJH", "/NJS", "/NP", "/NFL", "/NDL"], { windowsHide: true })
  else cpSync(exe, join(aside, basename(exe)))
  if (!existsSync(join(aside, basename(exe)))) throw new Error(`could not copy ${exe}`)
  for (let i = 0; ; i++) {
    try {
      rmSync(exe, { force: true })
      renameSync(join(aside, basename(exe)), exe)
      break
    } catch (e) {
      if (i === 40) throw e
      Bun.sleepSync(100)
    }
  }
  rmSync(aside, { recursive: true, force: true })
  // Linux keeps a file written a moment ago in its page cache: dropped, so the launch reads the disk.
  if (process.platform === "linux") evict(exe)
}

/** Every page of the exe's image in memory, as after a launch a moment ago (Windows). A machine
 *  short of memory drops them between launches, and a "warm" launch then reads them again before
 *  `main`: 85-1,873 ms to `main` instead of 20 in 5 of 24 launches on a busy machine (exp_9fb654). On Linux the
 *  file is read into the page cache. */
function warmExe(exe: string) {
  if (process.platform === "linux") warm(exe)
  if (process.platform !== "win32") return
  const k32 = dlopen("kernel32.dll", {
    CreateFileW: { args: [FFIType.ptr, FFIType.u32, FFIType.u32, FFIType.ptr, FFIType.u32, FFIType.u32, FFIType.ptr], returns: FFIType.u64 },
    CreateFileMappingW: { args: [FFIType.u64, FFIType.ptr, FFIType.u32, FFIType.u32, FFIType.u32, FFIType.ptr], returns: FFIType.u64 },
    MapViewOfFile: { args: [FFIType.u64, FFIType.u32, FFIType.u32, FFIType.u32, FFIType.u64], returns: FFIType.ptr },
    UnmapViewOfFile: { args: [FFIType.ptr], returns: FFIType.i32 },
    CloseHandle: { args: [FFIType.u64], returns: FFIType.i32 },
  }).symbols
  // SizeOfImage, from the PE header.
  const first = new Uint8Array(4096)
  const fd = openSync(exe, "r")
  readSync(fd, first, 0, 4096, 0)
  closeSync(fd)
  const head = new DataView(first.buffer)
  const size = head.getUint32(head.getUint32(0x3c, true) + 24 + 56, true)
  const file = k32.CreateFileW(ptr(new Uint8Array(Buffer.from(exe + "\0", "utf16le"))), 0x80000000, 7, null, 3, 0, null)
  const section = k32.CreateFileMappingW(file, null, 0x02 | 0x1000000, 0, 0, null)
  const view = section ? k32.MapViewOfFile(section, 0x0004, 0, 0, 0n) : null
  if (view) {
    const bytes = new Uint8Array(toArrayBuffer(view, 0, size))
    let sum = 0
    for (let at = 0; at < size; at += 4096) sum += bytes[at]!
    void sum
    k32.UnmapViewOfFile(view)
  }
  if (section) k32.CloseHandle(section)
  k32.CloseHandle(file)
}

/** Desktop's `userData` folder as a first launch finds it (`desktop-profile.ts`): made once per size
 *  (OPENCODE_LAUNCH_DESKTOP_MB, 64 by default: a file that has held a few pasted screenshots), then
 *  copied to where the app looks for it under its root (`<root>/config/ai.opencode.desktop`). */
function desktop(home: string, running: boolean) {
  const mb = Number(process.env.OPENCODE_LAUNCH_DESKTOP_MB ?? 64)
  const dir = join(SCRATCH, "desktop", `${mb}mb${running ? "-running" : ""}`)
  if (!existsSync(join(dir, "drafts.sqlite"))) desktopProfile(dir, { mb, running, session: SESSION })
  cpSync(dir, join(home, "config", "ai.opencode.desktop"), { recursive: true })
}

/** What the updater does to an exe it has staged, and the setup to the one it installed (Windows): a process is
 *  made from it, suspended, and ended before a line of it runs. A virus scanner checks a new exe the first time a
 *  process is made from it, inside that call; this pays the check before the timed launch, as an update or an
 *  install by a build with that step does. Asked for by a file named `staged` beside the exe, so one build can be
 *  both sides: a copy without the file is the launch after an update that left the check to the person's start. */
function staged(exe: string) {
  if (process.platform !== "win32" || !existsSync(join(dirname(exe), "staged"))) return
  const { dlopen, FFIType, ptr } = require("bun:ffi")
  const k32 = dlopen("kernel32.dll", {
    CreateProcessW: { args: [FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.i32, FFIType.u32, FFIType.ptr, FFIType.ptr, FFIType.ptr, FFIType.ptr], returns: FFIType.i32 },
    TerminateProcess: { args: [FFIType.u64, FFIType.u32], returns: FFIType.i32 },
    WaitForSingleObject: { args: [FFIType.u64, FFIType.u32], returns: FFIType.u32 },
    CloseHandle: { args: [FFIType.u64], returns: FFIType.i32 },
  }).symbols
  const wide = (s: string) => new Uint8Array(Buffer.from(s + "\0", "utf16le"))
  const si = new Uint8Array(104)
  new DataView(si.buffer).setUint32(0, 104, true)
  const pi = new Uint8Array(24)
  // CREATE_SUSPENDED | CREATE_NO_WINDOW, as `features/updater/src/windows.rs` `first_start`.
  if (!k32.CreateProcessW(ptr(wide(exe)), ptr(wide(`"${exe}"`)), null, null, 0, 0x4 | 0x0800_0000, null, ptr(wide(dirname(exe))), ptr(si), ptr(pi))) throw new Error(`could not make a process from ${exe}`)
  const view = new DataView(pi.buffer)
  k32.TerminateProcess(view.getBigUint64(0, true), 0)
  k32.WaitForSingleObject(view.getBigUint64(0, true), 5000)
  k32.CloseHandle(view.getBigUint64(8, true))
  k32.CloseHandle(view.getBigUint64(0, true))
}

/** Before every launch, never timed: the server up, the home seeded, the exe made cold or warm. */
async function before(ctx: Before) {
  const shape = ctx.target as Shape
  if (!ctx.home || !ctx.env || !ctx.exe) throw new Error("this bench needs an exe variant: --head exe:<path to opencode-desktop>")
  await server(shape)
  folders(ctx.home)
  if (ctx.case.startsWith("first-desktop")) desktop(ctx.home, ctx.case === "first-desktop-running")
  else if (ctx.case !== "first") {
    // `OPENCODE_LAUNCH_PROFILE_BY`: another build's exe makes the profile, for the launch after an update from it.
    const by = ctx.case === "first-new-build" ? (process.env.OPENCODE_LAUNCH_PROFILE_BY ?? ctx.exe) : ctx.exe
    const tpl = await template(by, ctx.env, ctx.home, shape)
    // A new build finds the profile and none of what the last exe kept for itself (its page list, compiled shaders).
    for (const name of readdirSync(tpl)) if (name !== "ready" && name !== "home" && name !== "tmp" && !(ctx.case === "first-new-build" && name === "cache")) cpSync(join(tpl, name), join(ctx.home, name), { recursive: true })
  }
  if (ctx.case === "cold" || ctx.case === "first-new-build") {
    images(ctx.exe).forEach(coldExe)
    staged(ctx.exe)
  } else if (process.env.OPENCODE_LAUNCH_WARM_EXE !== "0") images(ctx.exe).forEach(warmExe)
}

// Each milestone is a line the app prints with the mark's own time (epoch ms, to the microsecond in builds from 24 Nov).
const mark = (name: string) => ({ match: new RegExp(`launch-mark ${name.replace(/\./g, "\\.")} \\d+ at=(?<at>\\d{13}(?:\\.\\d+)?)`), in: "output" as const })
const milestones = {
  main: mark("main.entered"),
  window: mark("window.created"),
  first_frame: mark("frame.first.presented"),
  input_ready: mark("input.ready"),
  server: mark("server.answered"),
  content_live: mark("content.live"),
}
// `main.entered` and `window.created` are printed by builds from 24 Nov on: OPENCODE_LAUNCH_EARLY=0 leaves them out for an older one.
const early = process.env.OPENCODE_LAUNCH_EARLY === "0" ? [] : ["launch.main", "launch.window"]
const metrics = [...early, "launch.input_ready", "launch.server", "launch.content_live", "launch.painted", "cpu.work"]
const app = (shape: Shape, name: string) => native("opencode-rune", { name, output: true, viewport: { width: 1280, height: 800 }, args: ["--server", serverUrl(shape)] })
const launch = (profile: "fresh" | "warm") => ({ until: { milestone: "content_live" }, milestones, profile, before })

export default bench({
  targets: {
    none: app("none", "Brocode, no profile"),
    small: app("small", "Brocode, small profile"),
    big: app("big", "Brocode, big profile"),
  },
  cases: {
    first: { primary: "launch.first_frame", hero: true, targets: ["none"], gpuCache: "cold", metrics, launch: launch("fresh") },
    "first-desktop": { primary: "launch.first_frame", targets: ["none"], gpuCache: "cold", metrics, launch: launch("fresh") },
    "first-desktop-running": { primary: "launch.first_frame", targets: ["none"], gpuCache: "cold", metrics, launch: launch("fresh") },
    "first-new-build": { primary: "launch.first_frame", targets: ["small", "big"], gpuCache: "cold", metrics, launch: launch("fresh") },
    cold: { primary: "launch.first_frame", targets: ["small", "big"], gpuCache: "cold", metrics, launch: launch("fresh") },
    warm: { primary: "launch.first_frame", targets: ["small", "big"], metrics, launch: launch("fresh") },
  },
})
