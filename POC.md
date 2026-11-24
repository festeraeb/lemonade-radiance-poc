# Lemonade + radiance: proof of concept

A proof-of-concept fork, not a Lemonade release. It runs
[radiance](https://codeberg.org/StillDeadcode/radiance) (AMD HIP inference server) as a Lemonade backend
through a JSON backend definition, with no new C++ in lemond.

## What is here

- **lemond**: Lemonade with [#3906](https://github.com/lemonade-sdk/lemonade/pull/3906) (JSON backend definitions,
  head a0ba03e7d) and `tools/backend_conformance.py` (grades lifecycle, eviction, device, telemetry).
- **`engine-adapters/`**: engine specs turned into `backends.d` definitions (`tools/to_backend_definition.py`), and
  `src/bin/radiance-lemonade.rs`, the launcher radiance needs.
- **`radiance/`**: patches that build radiance's RDNA3 kernel library (libr3d) for RDNA2 (gfx103x), `build.sh`, and
  [UPSTREAM-BUG.md](radiance/UPSTREAM-BUG.md), a radiance bug not yet reported.
- **`poc/radiance-lemond.sh`**: starts a throwaway lemond with the radiance definition.

## Results (2026-11-24)

Through lemond, `tools/backend_conformance.py`, MiniCPM5-2B bf16:

| Where | L1 lifecycle | L2 eviction | L3 device | L4 telemetry | Earned |
|---|---|---|---|---|---|
| RX 6700 XT (gfx1031), lemond built on that host (Arch) | pass, 20.8 s, 12 GB on GPU 0 freed on unload | pass | pass: `--device 0`, memory on GPU 0 only | pass, 109 metric families | **L4** |
| CPU host build, 8 cores (Ubuntu) | pass, 4.4 GB freed on unload | pass | not tested (no AMD card) | pass | L2 |

Same container, radiance alone, greedy, 32 tokens:

| | 8 cores, Xeon Gold 6226 | RX 6700 XT (gfx1031, patched libr3d) |
|---|---|---|
| first token | 9.0 s | 0.22 s |
| decode | 0.8 tok/s | 9.3 tok/s |

- GPU output equals the CPU build for 27 of 32 tokens, then parts at a near-tie (logprob -1.612 vs -1.597).

## Run it

```bash
cmake --preset default -DBUILD_TESTING=OFF && cmake --build --preset default --target lemond lemonade
cargo build --release --manifest-path engine-adapters/Cargo.toml --bin radiance-lemonade
bash radiance/build.sh gfx1031 ./radiance-src          # or "none" for the CPU build
curl -LO https://huggingface.co/StillDeadcode/minicpm5-2b-bf16/resolve/main/minicpm5-2b-bf16.rad
bash poc/radiance-lemond.sh minicpm5-2b-bf16.rad ./radiance-src/build
```

## How radiance is wired

- radiance reads its settings only from a YAML file. `radiance-lemonade` takes the definition's flags, writes the file
  and `exec`s radiance, so lemond holds the server's PID.
- radiance answers 404 to any model id but its own: `{model_name}` passes `/model/name`, and each request's `/model`
  is rewritten to it.
- `{ctx}` now sets `metadata.uses_ctx_size`; without it lemond has no `/options/ctx_size` and the load fails.
- `--spec-tokens 0`: this radiance build refuses block-FP8 drafters.

## Limits

- Graded on one AMD card (RX 6700 XT); other gfx103x and gfx11/gfx12 cards untested here.
- The RDNA2 replacement for the 16x16x16 wave-matrix ops is correct, not fast. The test host's small PCI BAR also
  likely caps the launch rate.
- MiniCPM5-2B IQ8R does not load: its `rmsnorm_had_quant_i8g` op exists only in the RDNA4 and Strix Halo libraries.
- Based on #3906 as of 2026-11-13, not current Lemonade main.

## Licenses

Lemonade: Apache-2.0 (`LICENSE`). radiance: Apache-2.0 (`radiance/LICENSE.radiance`, `radiance/NOTICE.radiance`);
the patches are under the same license.
