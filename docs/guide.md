# AXL guide

AXL is a robot workshop on your computer. You build a robot from brick parts, write a program for
it in C or Rust, and watch it run on real physics. This guide takes you from the first launch to
your first passed mission in about half an hour.

## 1. Start

Unpack the package for your system and run `axl` (`AXL.app` on macOS). The first time, AXL runs a
short test of your graphics card and picks a quality level (change it any time in Settings).

Open a mission and its screen with the command line while the menus are being finished:

```sh
axl --code m01 --code-run      # mission 1: the code editor, the brief, the Rover
axl --builder                  # build a robot from parts
axl --sandbox rover            # watch the Rover with the time controls and the plotter
```

## 2. Your first mission (m01, First Drive)

1. Read the brief in the Mission panel. It says what the Rover has to do and lists the exact
   success criteria.
2. Make a prediction. Missions with a prediction question ask for your guess before your first
   run. It is not marked: it is there so you can compare it with what the simulation shows.
3. Write your program in the editor. Press **Build and Run** to watch it drive the Rover, or
   **Check my solution on every seed** to grade it. Grading runs your program on every grading
   seed (small changes in battery charge and sensor noise) and the Rover has to succeed on all of
   them.
4. Read the Results: pass or fail for each criterion, with numbers; time, energy and collisions;
   stars; a replay of the path (drag the slider); and a hint if you did not pass.

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

## 7. Accuracy

AXL's physics and hardware models are checked against textbook answers by 18 automated tests
(`cargo xtask validate`). The latest report ships with the release (`validation-report`) and is
shown under Settings, About.

## 8. Where files live

Your data folder (`AXL_DATA_DIR` overrides it) holds `settings.ron`, `progress.ron`, your robots
(`robots/*.axlbot`) and your programs (`projects/<mission>/<c|rust>/`). Each file has a `.bak`
copy of its previous version.

## 9. Building from source

See the README: `cargo xtask fetch-mujoco`, then `cargo run -p axl-app --release`.
