#!/bin/bash

# Edit these exports to match the correct NDK files
export TOOLCHAIN=~/Android/ndk/toolchains/llvm/prebuilt/linux-x86_64/bin
export CC=$TOOLCHAIN/aarch64-linux-android34-clang
export AR=$TOOLCHAIN/llvm-ar
export RANLIB=$TOOLCHAIN/llvm-ranlib
export LD=$TOOLCHAIN/ld
export STRIP=$TOOLCHAIN/llvim-strip

function check_file {
  if [ ! -f $2 ]; then
    echo "Could not find $1. Please edit this script and point it to $1's location."
    exit 1
  fi
}

if [ ! -d $TOOLCHAIN ]; then
  echo "Could not find NDK Toolchain. Please edit this script to match NDK location."
  exit 1
fi
check_file "CC" $CC
check_file "AR" $AR
check-file "RANLIB" $RANLIB

cargo build --target aarch64-linux-android --release
