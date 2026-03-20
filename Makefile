TRIPLE   = i686-pc-windows-msvc
XWIN_SDK = $(CURDIR)/xwinSDK

# The cc crate (used by minhook and rusqlite) invokes the C compiler with
# its working directory set to the build output dir, so xwinSDK include
# paths must be absolute. Makefile provides $(CURDIR) for this purpose.
export CC_i686_pc_windows_msvc     := clang-cl
export CFLAGS_i686_pc_windows_msvc := \
    -imsvc $(XWIN_SDK)/crt/include \
    -imsvc $(XWIN_SDK)/sdk/include/ucrt \
    -imsvc $(XWIN_SDK)/sdk/include/um \
    -imsvc $(XWIN_SDK)/sdk/include/shared

.PHONY: all release debug clean

all: release

release:
	cargo build --release --target $(TRIPLE)

debug:
	cargo build --target $(TRIPLE)

clean:
	cargo clean
