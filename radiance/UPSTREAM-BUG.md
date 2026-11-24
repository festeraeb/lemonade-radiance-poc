# radiance bug: libr3d refused next to libref at HEAD

Not filed upstream: we have no Codeberg account. If you have one, please file this at
https://codeberg.org/StillDeadcode/radiance/issues (text below is ready to paste).

---

**Title:** libr3d: `gated_had_quant_i8` schema lags libref since ade49d3, loader refuses libr3d

- Commit: 5e0cd1e (main, 2026-11-24).
- ade49d3 added optional `gate_cap` / `up_cap` to `gated_had_quant_i8` in libref, libavx and libr4d (8 params).
- libr3d still declares 6 (`pGatedHadQuantI8`, `libr3d/r3d_rows.cpp`).
- Result: with libr3d and libref both loaded, one is refused whole:
  - `--kernels libr3d:libref`: `op 'gated_had_quant_i8' has two different schemas: 'libr3d' fixed it first, 'libref' disagrees (6 parameters vs 8) -- refusing 'libref'`
  - `--kernels libref:libr3d`: same, `refusing 'libr3d'`.
- Seen with `rad-kbench` replay; affects any gfx11 card.
- Fix we used: add `P_F64O("gate_cap"), P_F64O("up_cap")` to `pGatedHadQuantI8`, and return `RAD_E_UNSUPPORTED`
  from `r3d_rad_gated_had_quant_i8` when either is non-zero (no softcap in that kernel).
  Patch: `patches/0003-libr3d-gated_had_quant_i8-takes-gate_cap-up_cap-refu.patch` in this repo.

---

Also seen, smaller (may not be worth an issue):

- On a card with a small PCI BAR (no Resizable BAR), `r3d_selftest` (`build_gdn_kkt`) and `rad-kbench`
  (`rad_gemm_run`, from `r3d_rad_logits_gemm_bf16`) crash with SIGSEGV: host code reads or writes a device address.
  The server itself runs on the same card.
