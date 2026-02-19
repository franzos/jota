import * as esbuild from "esbuild";

const watch = process.argv.includes("--watch");

const shared = {
  bundle: true,
  sourcemap: false,
  minify: !watch,
  target: "es2022",
  outdir: "dist",
};

// wallet.ts — injected into page context as IIFE
const walletBuild = esbuild.build({
  ...shared,
  entryPoints: ["src/wallet.ts"],
  format: "iife",
  globalName: "IotaWalletBridge",
  ...(watch ? { plugins: [watchPlugin("wallet")] } : {}),
});

// background.ts — service worker (ESM)
const backgroundBuild = esbuild.build({
  ...shared,
  entryPoints: ["src/background.ts"],
  format: "esm",
  ...(watch ? { plugins: [watchPlugin("background")] } : {}),
});

// content.ts — content script (IIFE)
const contentBuild = esbuild.build({
  ...shared,
  entryPoints: ["src/content.ts"],
  format: "iife",
  ...(watch ? { plugins: [watchPlugin("content")] } : {}),
});

function watchPlugin(name) {
  return {
    name: `watch-${name}`,
    setup(build) {
      build.onEnd((result) => {
        const time = new Date().toLocaleTimeString();
        if (result.errors.length > 0) {
          console.error(`[${time}] ${name}: build failed`);
        } else {
          console.log(`[${time}] ${name}: build succeeded`);
        }
      });
    },
  };
}

await Promise.all([walletBuild, backgroundBuild, contentBuild]);
console.log("Build complete.");
