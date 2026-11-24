# radiance patches

Against radiance 5e0cd1e (codeberg.org/StillDeadcode/radiance, Apache-2.0).

- `0001`: libr3d builds for gfx103x. `r3d_wmma_rdna2.h` gives the gfx11 16x16x16 WMMA results with ds_bpermute and
  v_dot2 / v_dot4, same fragment layout; empty on other targets.
- `0002`: gfx10 spellings for three inline-asm scale loads (`global_load_ushort`, no `.l` operand).
- `0003`: `gated_had_quant_i8` schema matches libref again (see [UPSTREAM-BUG.md](UPSTREAM-BUG.md)).

```bash
bash radiance/build.sh gfx1031 ./radiance-src     # ROCm 7.2; "none" for the CPU host backend
```

Tested on an RX 6700 XT (gfx1031), ROCm 7.2. Other gfx103x chips are untested.
