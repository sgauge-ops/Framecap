#!/bin/sh

env VK_LAYER_PATH="$PWD/layer" \
    VK_INSTANCE_LAYERS=VK_LAYER_BC250_framecap \
    furmark --demo furmark-vk
