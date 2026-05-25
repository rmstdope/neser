# CasualPokePlayer GBEmulatorShootout ROMs

This directory contains CasualPokePlayer test ROMs mirrored by
GBEmulatorShootout:

<https://github.com/gbdev/GBEmulatorShootout/tree/main/testroms/cpp>

The ROMs are used by `src/gb/integration_tests/cpp_tests.rs` to automate the
upstream DMG Shootout rows and NESER-specific CGB coverage for the same ROMs.
The vendored files are from GBEmulatorShootout commit
`f2e95de5ae2293fdf07887b2f79e7f79baa9c63e`.

GBEmulatorShootout and CasualPokePlayer's test ROMs are distributed under the
MIT license. Each binary is 32 KiB. File SHA-256 checksums:

```text
353c72be3227f6dcd85939b54efcf31b745c20beb821a1d370eb8a7797f47bd1  rtc-invalid-banks-test.gb
e7a4df7816a7eb7c1c7328e4f58a88f0a6611104d2ac2590c03826021a558e60  rtc-invalid-banks-test.png
2df0db7b8cc5719a208bd59a2a0e3096cc278fe8614c0ee986b587e7803bffa5  latch-rtc-test.gb
268f295d7f530e4f3c725e0d6d8fac53dbafd696659faffe40b8694658e96193  latch-rtc-test.png
40117ff683b2bae07a0623ae86a7c8275f9aa23d75c008816030e8ccbf4b6405  ramg-mbc3-test.gb
0108c6759326505814b3f9555231e25ff6c636c54025c27837845028c1d5c290  ramg-mbc3-test.png
```
