# ax6 rtc3test

This directory contains the split ax6 MBC3 RTC test ROMs mirrored by
GBEmulatorShootout:

<https://github.com/gbdev/GBEmulatorShootout/tree/main/testroms/ax6>

The ROMs are used by `src/gb/integration_tests/ax6_tests.rs` to run the Basic,
Range, and Sub-second writes result screens directly on both DMG and CGB
hardware modes. The vendored files are from GBEmulatorShootout commit
`0464f9077abd7355943d47654d4970af33c8527b`; each binary is 32 KiB with
SHA-256:

```text
5862a567c4799ce87e64d699420daa59d216744781a517965e074ed399dd56d0  rtc3test-1.gb
a50f2da808feff4de8f05067af34e4e444e35a4303e7fc636656e58a4ccea90c  rtc3test-2.gb
38a004276ad0d90670dc7cd00ebbd85b7eac04f54a8f244360ae5092e5aefa76  rtc3test-3.gb
```
