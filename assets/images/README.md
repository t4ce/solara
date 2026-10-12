cat.jpg is copied unchanged from TRUEOS/vendor/limine/test/bg.jpg, the existing Limine test background (1152 × 768 baseline JPEG). Used here as one shared image to demonstrate layout, texture reuse, object-fit and scrolling.

The home page reads the kernel’s embedded 3840 × 2160 TRUEOS logo through
`trueos::os_assets::logo_jpeg()`. Solara does not embed a second copy. Its `object-fit: contain` home-page layout fills the available frame in one
dimension, preserving the source aspect ratio.
