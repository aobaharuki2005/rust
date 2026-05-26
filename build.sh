#!/bin/zsh

### Setting up environment variables
# export RUSTC_WRAPPER=/Users/test/.mozbuild/sccache/sccache
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX14.sdk
export MACOSX_DEPLOYMENT_TARGET=11.0    # for cross-compiling build only
export MACOSX_STD_DEPLOYMENT_TARGET=11.0    # for cross-compiling build only
export LDFLAGS="-ld_classic"
# export LIBRARY_PATH="${LIBRARY_PATH}:/opt/local/lib"

### Check if environment variables have been correctly set up
echo "### Build environment variables: "
printenv | grep -E "SDKROOT|MACOSX|LDFLAGS"

### Shallow synchronize submodules
echo "====== STEP 1: Fetch submodules ====="
echo "### Fetching ..."
git submodule update --depth 1 --init
echo "### Checking if llvm-project has been checked out correctly..."
cd src/llvm-project
git log -1 --oneline
# Come back to the original directory
cd ../..

### Build Rustc and cargo
echo "====== STEP 2: Build Rustc and cargo ====="
echo "### Symlink the configuration"
ln -sf ./config-legacy.toml ./config.toml
echo "### Here is the build configuration:"
cat ./config.toml
echo "### Starting to build ..."
python3 ./x.py build

### Install and package
echo "====== STEP 3: Install and package ====="
python3 ./x.py install
cd build
zip -qr 1.74.0-custom-aarch64.zip ./1.74.0-custom-aarch64
