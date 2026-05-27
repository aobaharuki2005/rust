#!/bin/zsh

### Setting up environment variables
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX15.sdk
# export MACOSX_DEPLOYMENT_TARGET=11.0  # only needed if building targeted Rust toolchain
# export MACOSX_STD_DEPLOYMENT_TARGET=11.0  # only needed if building targeted Rust toolchain
export LDFLAGS="-ld_classic"
export VERSION=$(cat src/version)

### Check if environment variables have been correctly set up
echo "### Build environment variables: "
printenv | grep -E "SDKROOT|MACOSX|LDFLAGS|VERSION"

### Shallow synchronize submodules
echo "====== STEP 1: Fetch submodules ====="
echo "### Fetching ..."
git submodule update --depth 1 --init --recursive

### Build Rustc and cargo
echo "====== STEP 2: Build Rustc and cargo ====="
echo "### Symlink the configuration"
ln -sf ./config-legacy.toml ./config.toml
echo "### Here is the build configuration:"
cat ./config.toml
echo "### Starting to build ..."
/usr/bin/python3 ./x.py build

### Install and package
echo "====== STEP 3: Install and package ====="
/usr/bin/python3 ./x.py install
cd build

zip -qr $VERSION-custom-aarch64.zip ./$VERSION-custom-aarch64
