TRIPLE   = i686-pc-windows-msvc
XWIN_SDK = $(CURDIR)/xwinSDK

# Both the C compiler (cc crate) and the linker (lld-link) are invoked with
# their working directory set to the build output dir, so all xwinSDK paths
# must be absolute. Makefile provides $(CURDIR) for this purpose.
export CC_i686_pc_windows_msvc     := clang-cl
export CFLAGS_i686_pc_windows_msvc := \
    -imsvc $(XWIN_SDK)/crt/include \
    -imsvc $(XWIN_SDK)/sdk/include/ucrt \
    -imsvc $(XWIN_SDK)/sdk/include/um \
    -imsvc $(XWIN_SDK)/sdk/include/shared
export RUSTFLAGS := \
    -Clinker-flavor=msvc \
    -Lnative=$(XWIN_SDK)/crt/lib/x86 \
    -Lnative=$(XWIN_SDK)/sdk/lib/ucrt/x86 \
    -Lnative=$(XWIN_SDK)/sdk/lib/um/x86

.PHONY: all release debug clean

all: release

release:
	cargo build --release --target $(TRIPLE)

debug:
	cargo build --target $(TRIPLE)

clean:
	cargo clean
