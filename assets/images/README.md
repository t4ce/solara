cat.jpg is copied unchanged from TRUEOS/vendor/limine/test/bg.jpg, the existing Limine test background (1152 × 768 baseline JPEG). Used here as one shared image to demonstrate layout, texture reuse, object-fit and scrolling.

logo.jpg is a 640 × 360 JPEG derivative of the existing repository-root logo.jpg (3840 × 2160), resized with ffmpeg `-vf scale=640:360 -q:v 3`. The bundled home page uses it through the same kernel JPEG decode and native texture path, keeping its decoded allocation small.
