env VK_LAYER_PATH="$PWD/layer" \
    VK_INSTANCE_LAYERS=VK_LAYER_BC250_framecap \
    vkcube --c 3000 --wsi wayland --present_mode 0
