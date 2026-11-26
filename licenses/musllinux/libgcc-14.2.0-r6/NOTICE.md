# Alpine libgcc in SQLAero musllinux wheels

The musllinux x86_64 and aarch64 wheels bundle `libgcc_s.so.1` from
Alpine Linux v3.22, package `libgcc 14.2.0-r6`. Wheel repair assigns a
hash-based name and adjusts the ELF metadata. The source package and
both original Alpine library hashes are recorded in [SOURCE.json](SOURCE.json).

GCC runtime copyright is held by the Free Software Foundation, Inc.
GCC 14.2.0 carries copyright notices for 1989–2024 in
[`libgcc2.c`](https://github.com/gcc-mirror/gcc/blob/releases/gcc-14.2.0/libgcc/libgcc2.c)
and 1997–2024 in
[`unwind-dw2.c`](https://github.com/gcc-mirror/gcc/blob/releases/gcc-14.2.0/libgcc/unwind-dw2.c).
The [AArch64 atomic routines](https://github.com/gcc-mirror/gcc/blob/releases/gcc-14.2.0/libgcc/config/aarch64/lse.S)
carry the 2019–2024 notice and credit Linaro Ltd. The complete source
retains the individual notices.

These runtime sources use GPL version 3 or later with the GCC Runtime
Library Exception version 3.1. See [GPL 3](COPYING3) and
[the exception](COPYING.RUNTIME) for the terms and conditions. The
broader Alpine GCC package also declares GPL version 2 or later and
LGPL version 2.1 or later. Its original [GPL 2](COPYING) and
[LGPL 2.1](COPYING.LIB) texts are provided with the package provenance.

The [original GCC source tarball](https://gcc.gnu.org/pub/gcc/releases/gcc-14.2.0/gcc-14.2.0.tar.xz),
the [pinned Alpine build directory and patches](https://github.com/alpinelinux/aports/tree/fbf60319be3bbaf6dd32ef55cc6fb7189e05c266/main/gcc),
and the included [Alpine APKBUILD](APKBUILD) identify the source used for both architectures.
