# AXL guide

AXL is a robot workshop on your computer. You build a robot from brick parts, write a program for
it in C or Rust, and watch it run on real physics. This guide takes you from the first launch to
your first passed mission in about half an hour.

## 1. Start

Unpack the package for your system and run `axl` (`AXL.app` on macOS). The first time, AXL runs a
short test of your graphics card and picks a quality level (change it any time in Settings).

AXL has seven screens. The bar along the top switches between them, and so do Alt+1 to Alt+7.

| Screen | Alt+ | What it is for |
| --- | --- | --- |
| Home | 1 | **Continue** (the mission you had open last, or the first one you have not passed), Missions, Sandbox, Builder, Settings, and how many missions and stars you have so far. |
| Missions | 2 | All 12 missions in four chapters, each with its status (New, Tried, Passed), stars and the ideas it teaches. Click a mission for the brief, the goal, the rules, the starter robot and the exact success criteria, then **Open this mission**. |
| Builder | 3 | Build a robot from parts. **Simulate** drives your design on an open floor. |
| Code | 4 | The editor, the brief, the API reference and the build log, with the mission's world beside them. |
| Simulate | 5 | The 3D view with the time controls, overlays, inspector and plotter. In the Sandbox it also has a slider for every motor. |
| Results | 6 | The verdict of your last check. |
| Settings | 7 | Quality, units, grid opacity, UI scale, invert orbit, toolchain paths and the data folder. **About** shows the version and the accuracy report. |

You can also open a screen from the command line:

```sh
axl --code m01 --code-run      # mission 1: the code editor, the brief, the Rover
axl --screen missions          # or: home, builder, code, simulate, results, settings, about
axl --builder                  # build a robot from parts
axl --sandbox rover            # watch the Rover with the time controls and the plotter
```

## 2. Your first mission (m01, First Drive)

1. On Home, press **Start mission m01** (or open it from Missions). The Code screen opens with
   the brief, the editor, and the mission's world with the Rover standing in it.
2. Read the brief in the Mission panel. It says what the Rover has to do and lists the exact
   success criteria.
3. Make a prediction. Missions with a prediction question ask for your guess before your first
   run. It is not marked: it is there so you can compare it with what the simulation shows.
4. Write your program in the editor. Press **Build and Run** to watch it drive the Rover in the
   mission's world (the toolbar shows the seed; it is the same world and seed as the first graded
   run), or **Check my solution on every seed** to grade it. Grading runs your program on every
   grading seed (small changes in battery charge and sensor noise) and the Rover has to succeed
   on all of them.
5. The Results screen opens: pass or fail for each criterion, with numbers; time, energy and
   collisions; stars; a replay of the path (drag the slider); **Watch seed N in 3D**, which runs
   your program again in the world of that seed; and a hint if you did not pass.

Hints unlock after your first run that does not pass. There are three, from a nudge to nearly the
answer. Using one costs the "no hints" star, but never the pass.

## 3. Stars

Every mission has up to three stars: one for passing, and two more for harder goals that are
written in the brief (for example "heading within 1 degree"). Stars are kept in `progress.ron` in
your data folder.

## 4. Warm-ups

Before a mission AXL may ask up to three short questions about ideas from earlier missions, with
new numbers each time. Ideas you remember well come back less often (the schedule is FSRS-4.5).
Wrong answers are not penalised; the working is shown after each answer.

## 5. Programming

The API is in `sdk/c/axl.h` (C) and `sdk/rust/axl-robot` (Rust), and the Code screen has a
searchable API reference. The programs in the templates are the starting points; build errors are
clickable and jump to the line.

C needs wasi-sdk; Rust needs the `wasm32-wasip1` target (`rustup target add wasm32-wasip1`). The
toolchain chip in the Code toolbar tells you what is missing and how to fix it.

## 6. The simulation

- Time controls: Space (pause), `.` (step 1 ms), Shift + `.` (step 10 ms), `[` and `]` (speed
  0.1x to 4x), Ctrl+R (reset). The scrub bar rewinds by replaying.
- Overlays 1 to 7: frames, balance, contacts, sensors, trails, drawings, X-ray. The balance
  overlay shows a tick (the centre of mass is inside the support polygon) or a cross, so it never
  relies on colour alone.
- The plotter shows what your program sends with `axl_plot` and the built-in channels, and can
  export a CSV file.
- Camera: right-drag orbits, middle-drag pans, the wheel zooms, `F` focuses, `V` cycles the
  camera, `Home` resets.

## 7. Reading the screen

- The UI scales from 100% to 200%: Settings, the bar above the 3D view, or Ctrl and plus, Ctrl and
  minus, Ctrl and 0.
- Every action has a key or a visible button. The keys are in the tooltips. Status is always
  written as a word as well as a colour (Passed, Tried, PASS, FAIL), and the balance overlay shows
  a tick or a cross.

## 8. Accuracy

AXL's physics and hardware models are checked against textbook answers by 18 automated tests
(`cargo xtask validate`). The latest report ships with the release (`validation-report`) and is
shown under Settings, About, and in the package as `accuracy-report.md`.

## 9. Where files live

Your data folder (`AXL_DATA_DIR` overrides it) holds `settings.ron`, `progress.ron`, your robots
(`robots/*.axlbot`) and your programs (`projects/<mission>/<c|rust>/`). Each file has a `.bak`
copy of its previous version.

## 10. Building from source

See the README: `cargo xtask fetch-mujoco`, then `cargo run -p axl-app --release`.
