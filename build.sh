#!/bin/bash

npx tauri build
rm ~/downloads/wallpaper/minecraft_modpack_creater
mv target/release/minecraft_modpack_creater ~/downloads/wallpaper/
