// This is Human Code
// AI don't repair that

use anyhow::Context;
use reqwest::{Response};
use std::{collections::HashMap, sync::LazyLock};
use trie_rs::{Trie, TrieBuilder};

static BMCLAPI_TRIE: LazyLock<Trie<u8>> = LazyLock::new(|| {
    let mut result = TrieBuilder::new();
    result.push("https://maven.minecraftforge.net");
    result.push("https://piston-meta.mojang.com/mc/game/version_manifest.json");
    result.push("https://piston-meta.mojang.com/v1");
    // Vanilla manifest / version json / asset index
    result.push("https://launchermeta.mojang.com/mc/game/version_manifest.json");
    result.push("https://launchermeta.mojang.com/mc/game/version_manifest_v2.json");
    result.push("https://launchermeta.mojang.com");
    result.push("https://launcher.mojang.com");

    // Assets
    result.push("https://resources.download.minecraft.net");

    // Libraries
    result.push("https://libraries.minecraft.net");

    // Mojang Java
    result.push(
        "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
    );

    // Forge
    result.push("https://files.minecraftforge.net/maven");

    // LiteLoader
    result.push("https://dl.liteloader.com/versions/versions.json");

    // authlib-injector
    result.push("https://authlib-injector.yushi.moe");

    // Fabric
    result.push("https://meta.fabricmc.net");
    result.push("https://maven.fabricmc.net");

    // NeoForge
    result.push("https://maven.neoforged.net/releases/net/neoforged/forge");
    result.push("https://maven.neoforged.net/releases/net/neoforged/neoforge");

    // Quilt
    // Upstream API currently has bugs; temporarily unavailable.
    result.push("https://maven.quiltmc.org/repository/release");
    result.push("https://meta.quiltmc.org");

    result.build()
});

static BMCLAPI_REPLACE: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut result = HashMap::new();

    result.insert(
        "https://piston-meta.mojang.com/v1",
        "https://launchermeta.mojang.com/v1",
    );
    result.insert(
        "https://piston-meta.mojang.com/mc/game/version_manifest.json",
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json",
    );
    // Vanilla manifest / version json / asset index
    result.insert(
        "https://launchermeta.mojang.com/mc/game/version_manifest.json",
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json",
    );
    result.insert(
        "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest_v2.json",
    );
    result.insert(
        "https://launchermeta.mojang.com",
        "https://bmclapi2.bangbang93.com",
    );
    result.insert(
        "https://launcher.mojang.com",
        "https://bmclapi2.bangbang93.com",
    );

    // Assets
    result.insert(
        "https://resources.download.minecraft.net",
        "https://bmclapi2.bangbang93.com/assets",
    );

    // Libraries
    result.insert(
        "https://libraries.minecraft.net",
        "https://bmclapi2.bangbang93.com/maven",
    );

    // Mojang Java
    result.insert(
        "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
        "https://bmclapi2.bangbang93.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json",
    );

    // Forge
    result.insert(
        "https://files.minecraftforge.net/maven",
        "https://bmclapi2.bangbang93.com/maven",
    );

    result.insert(
        "https://maven.minecraftforge.net",
        "https://bmclapi2.bangbang93.com/maven",
    );

    // LiteLoader
    result.insert(
        "https://dl.liteloader.com/versions/versions.json",
        "https://bmclapi.bangbang93.com/maven/com/mumfrey/liteloader/versions.json",
    );

    // authlib-injector
    result.insert(
        "https://authlib-injector.yushi.moe",
        "https://bmclapi2.bangbang93.com/mirrors/authlib-injector",
    );

    // Fabric
    result.insert(
        "https://meta.fabricmc.net",
        "https://bmclapi2.bangbang93.com/fabric-meta",
    );
    result.insert(
        "https://maven.fabricmc.net",
        "https://bmclapi2.bangbang93.com/maven",
    );

    // NeoForge
    result.insert(
        "https://maven.neoforged.net/releases/net/neoforged/forge",
        "https://bmclapi2.bangbang93.com/maven/net/neoforged/forge",
    );
    result.insert(
        "https://maven.neoforged.net/releases/net/neoforged/neoforge",
        "https://bmclapi2.bangbang93.com/maven/net/neoforged/neoforge",
    );

    // Quilt
    // Upstream API currently has bugs; temporarily unavailable.
    result.insert(
        "https://maven.quiltmc.org/repository/release",
        "https://bmclapi2.bangbang93.com/maven",
    );
    result.insert(
        "https://meta.quiltmc.org",
        "https://bmclapi2.bangbang93.com/quilt-meta",
    );

    result
});

pub fn replace(origin_url: &str) -> String {
    let origin_url = origin_url.replacen("http://", "https://", 1);
    if let Some(prefix) = BMCLAPI_TRIE
        .common_prefix_search(&origin_url)
        .collect::<Vec<String>>()
        .last()
    {
        if let Some(bmcluri) = BMCLAPI_REPLACE.get(prefix.as_str()) {
            return bmcluri.to_string() + origin_url.strip_prefix(prefix).unwrap_or_default();
        }
    }
    origin_url.to_string()
}

pub async fn request(origin_url: &str) -> Result<Response, anyhow::Error> {
    let real_url = replace(origin_url);
    if let Ok(resp) = reqwest::get(real_url).await {
        if resp.status() != 200 {
            return reqwest::get(origin_url)
                .await
                .context("origin url request error");
        }
        Ok(resp)
    } else {
        reqwest::get(origin_url)
            .await
            .context("origin url request error")
    }
}

pub async fn fetch_json_value(origin_url: &str) -> Result<serde_json::Value, anyhow::Error> {
    let resp = request(origin_url).await?;
    resp.json::<serde_json::Value>().await.map_err(|e| e.into())
}

#[test]
fn test_replace() {
    assert_eq!(
        replace("https://piston-meta.mojang.com/mc/game/version_manifest.json"),
        "https://bmclapi2.bangbang93.com/mc/game/version_manifest.json"
    );
    assert_eq!(
        replace("http://resources.download.minecraft.net"),
        "https://bmclapi2.bangbang93.com/assets",
    );
    assert_eq!(
        replace("http://resources.download.minecraft.net/test"),
        "https://bmclapi2.bangbang93.com/assets/test",
    )
}
