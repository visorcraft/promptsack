/**
 * Package/tooling contract: LICENSE, branding, Node 24, multi-OS CI, Tauri stack.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dirname, '..');

function read(rel: string): string {
  return readFileSync(join(root, rel), 'utf8');
}

describe('repo contract', () => {
  it('LICENSE is GPL-3.0-only', () => {
    expect(existsSync(join(root, 'LICENSE'))).toBe(true);
    const license = read('LICENSE');
    expect(license).toMatch(/GNU GENERAL PUBLIC LICENSE/);
    expect(license).toMatch(/Version 3/);
    const pkg = JSON.parse(read('package.json'));
    expect(pkg.license).toBe('GPL-3.0-only');
  });

  it('package metadata brands PromptSack as Tauri app at 1.0.0', () => {
    const pkg = JSON.parse(read('package.json'));
    expect(pkg.name).toBe('promptsack');
    expect(pkg.version).toBe('1.0.0');
    expect(pkg.description.toLowerCase()).toContain('promptsack');
    expect(pkg.description.toLowerCase()).toMatch(/tauri|mongrel/);
    expect(pkg.engines.node).toMatch(/24/);
    expect(pkg.scripts.dev).toMatch(/tauri/);
    expect(pkg.scripts.dist).toMatch(/tauri/);
    expect(pkg.devDependencies['@tauri-apps/cli']).toBeTruthy();
    expect(pkg.devDependencies['@tauri-apps/api']).toBeTruthy();
    const cargo = read('src-tauri/Cargo.toml');
    expect(cargo).toMatch(/version\s*=\s*"1\.0\.0"/);
    const tauri = JSON.parse(read('src-tauri/tauri.conf.json'));
    expect(tauri.version).toBe('1.0.0');
  });

  it('GitHub Actions use Node 24, Rust, and build Windows, macOS, Linux', () => {
    const ci = read('.github/workflows/ci.yml');
    expect(ci).toMatch(/node-version:\s*\[?24\]?|node-version:\s*24/);
    expect(ci).toMatch(/ubuntu-latest/);
    expect(ci).toMatch(/macos-latest/);
    expect(ci).toMatch(/windows-latest/);
    expect(ci).toMatch(/rust-toolchain|dtolnay\/rust-toolchain/);
    expect(ci).toMatch(/tauri-action|tauri build|cargo test/);
    expect(ci).toMatch(/webkit2gtk|libwebkit2gtk/);
  });

  it('ships three-pane UI wired to Tauri bridge + Rust vault', () => {
    const html = read('src/renderer/index.html');
    expect(html).toMatch(/sidebar/);
    expect(html).toMatch(/list-pane|prompt-list/);
    expect(html).toMatch(/editor-pane|editor-form/);
    expect(html).toMatch(/PromptSack/);
    expect(html).toMatch(/Semantic search|search-input/);
    expect(html).toMatch(/bridge\.js/);

    const bridge = read('src/renderer/bridge.js');
    expect(bridge).toMatch(/__TAURI__/);
    expect(bridge).toMatch(/vault_open|vault_status/);
    expect(bridge).toMatch(/prompts_search/);
    expect(bridge).toMatch(/window\.promptsack/);

    const cargo = read('src-tauri/Cargo.toml');
    expect(cargo).toMatch(/mongreldb-kit/);
    expect(cargo).toMatch(/fastembed/);
    expect(cargo).toMatch(/keyring/);
    expect(cargo).toMatch(/tauri/);

    const store = read('src-tauri/src/vault/store.rs');
    expect(store).toMatch(/create_encrypted|open_encrypted/);
    expect(store).toMatch(/ann_search/);
    expect(store).toMatch(/MarkdownLibrary|markdown/);
    expect(store).toMatch(/smart_folders|SmartFolder/);

    const schema = read('src-tauri/src/vault/schema.rs');
    expect(schema).toMatch(/Embedding|embedding/);
    expect(schema).toMatch(/384/);
    expect(schema).toMatch(/Ann|ann/);
    expect(schema).toMatch(/prompt_index|smart_folders|ui_state/);

    const md = read('src-tauri/src/vault/markdown.rs');
    expect(md).toMatch(/parse_markdown|serialize_markdown/);

    const embedder = read('src-tauri/src/vault/embed.rs');
    expect(embedder).toMatch(/AllMiniLML6V2|MiniLM/);
    expect(embedder).toMatch(/384/);
    expect(embedder).toMatch(/OpenaiCompatible|openai-compatible/);

    const keyring = read('src-tauri/src/vault/store.rs');
    expect(keyring).toMatch(/keyring::Entry|KEYRING_SERVICE/);
  });

  it('no Electron runtime deps in package.json', () => {
    const pkg = JSON.parse(read('package.json'));
    const deps = { ...pkg.dependencies, ...pkg.devDependencies };
    expect(deps.electron).toBeUndefined();
    expect(deps['electron-builder']).toBeUndefined();
    expect(deps['@visorcraft/mongreldb']).toBeUndefined();
    expect(deps['@huggingface/transformers']).toBeUndefined();
  });

  it('README documents PromptSack + Tauri', () => {
    expect(existsSync(join(root, 'README.md'))).toBe(true);
    const readme = read('README.md');
    expect(readme).toMatch(/PromptSack/);
    expect(readme).toMatch(/MongrelDB|mongreldb/i);
    expect(readme).toMatch(/MiniLM|semantic/i);
    expect(readme).toMatch(/GPL-3\.0/);
    expect(readme).toMatch(/Tauri/i);
  });
});
