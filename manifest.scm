(use-modules (gnu packages commencement)
             (gnu packages pkg-config)
             (gnu packages freedesktop)
             (gnu packages xdisorg)
             (gnu packages vulkan)
             (gnu packages rust)
             (gnu packages linux)
             (gnu packages node)
             (gnu packages gtk)
             (gnu packages glib)
             (gnu packages tls))

(packages->manifest
 (list rust-1.88
       (list rust-1.88 "cargo")
       gcc-toolchain
       pkg-config
       wayland
       wayland-protocols
       libxkbcommon
       vulkan-loader
       eudev
       node
       gtk+
       glib
       openssl))
