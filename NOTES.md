# BUILD FLAG:
```sh
export SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX14.sdk
export MACOSX_DEPLOYMENT_TARGET=10.7
export MACOSX_STD_DEPLOYMENT_TARGET=10.7
export LDFLAGS="-ld_classic"
export RUSTFLAGS="-L /opt/local/lib -l static=zstd"
# export LIBRARY_PATH="${LIBRARY_PATH}:/opt/local/lib"
```

Other necessary useful commands:
```sh
# To reset the content of all submodules:
git submodule deinit -f --all
git clean -fdx 
git submodule update --init --recursive

# [allow(rustc::default_hash_types)]

# Update a patched crate
cargo update -p cc 

# Build
./x.py build --stage 2

# Install
./x.py install
```

# SYMBOL SUPPORT STATUS:

| # | Name | 1.74.0 | 1.75.0 |
| --- | --- | --- | --- |
| 1 | CCRandomGenerateBytes | /dev/urandom | √ with getrandom() alternative path |
| 2 | _clock_gettime | mach_absolute_time | √ |
| 3 | _fclonefileat | F | F |
| 4 | _fdopendir$INODE64 | √ | √ |
| 5 | _linkat, _openat, _unlinkat | √ | √ |
| 6 | _dirfd | x | x |

# BUILD NOTES:

## v1.74.0

1. `error: method `name_cstr` is never used`
```rust
error: method `name_cstr` is never used
   --> library/std/src/sys/unix/fs.rs:984:8
    |
821 | impl DirEntry {
    | ------------- method in this implementation
...
984 |     fn name_cstr(&self) -> &CStr {
    |        ^^^^^^^^^
    |
    = note: `-D dead-code` implied by `-D warnings`
```

**Solution:** add `#[allow(dead_code)]` before `fn name_cstr()`

2. `cc` need to be upgraded to `1.0.98` (patched) to ignore `"macOS deployment target too low, will be increased"` error
3. `#![allow(rustc::default_hash_types)]` must also be appended to `cc`'s `lib.rs` and `tools.rs` to enable use of old `HashMap` datatype


**Checklist before building patched Rust:**
1. Make sure paths in `config-legacy.toml` is correct
2. Make sure custom `cc` has been appended to both `/Cargo.toml` and `src/bootstrap/Cargo.toml`
3. Make sure `cc` crates has been checked out and patched
4. Make sure `0001_spec_base_apple_mod.rs.patch` (target cpu, dwarf version, macos minimum deployment target) has been properly set
5. Make sure `0002_unix_fs.rs.patch` has been patched
6. Make sure right stage0 `cargo` has been downloaded and put into same place as `rustc`
7. Make sure build environments has been properly set

**Checklist to established a macOS 10.11 host build environment:**
1. Install Xcode CLT ~~Xcode (full)~~
2. Install Macports, configure mirror
3. Install LLVM-17 (Rust 1.74.0)
4. Install Rust 1.73.0 (offline download)
5. Install MacOS 13 SDK (require gtar to extract)
6. Install new git (Macports) + download patched 1.74.0-custom
7. Install python3 to run `x` builder
8. Sets neccessary environment variable