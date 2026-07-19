/**
 * PromptSack ↔ Tauri bridge. Exposes window.promptsack for the renderer.
 */
async function inv(cmd, args = {}) {
  const core = window.__TAURI__?.core;
  if (core?.invoke) return core.invoke(cmd, args);
  throw new Error('Tauri IPC not available — open this UI inside the PromptSack app');
}

/** Drop undefined so partial updates don't wipe fields as null over IPC. */
function defined(obj) {
  const out = {};
  for (const [k, v] of Object.entries(obj)) {
    if (v !== undefined) out[k] = v;
  }
  return out;
}

window.promptsack = {
  vault: {
    status: () => inv('vault_status'),
    open: (passphrase) => inv('vault_open', { passphrase }),
    close: () => inv('vault_close'),
  },
  session: {
    unlock: (password, opts) =>
      inv('session_unlock', { password, remember: opts?.remember }),
    lock: (opts) => inv('session_lock', { forgetKeychain: opts?.forgetKeychain }),
    status: () => inv('session_status'),
  },
  prompts: {
    list: (filter, sort) => inv('prompts_list', { filter, sort }),
    get: (id) => inv('prompts_get', { id }),
    create: (input) =>
      inv('prompts_create', {
        input: {
          title: input.title,
          body: input.body ?? '',
          notes: input.notes ?? '',
          tags: input.tags ?? [],
          folderId: input.folderId ?? null,
          favorite: !!input.favorite,
          locked: !!input.locked,
        },
      }),
    update: (id, patch) =>
      inv('prompts_update', {
        id,
        patch: defined({
          title: patch.title,
          body: patch.body,
          notes: patch.notes,
          tags: patch.tags,
          folderId: patch.folderId,
          favorite: patch.favorite,
          locked: patch.locked,
        }),
      }),
    delete: (id) => inv('prompts_delete', { id }),
    search: (query) => inv('prompts_search', { query }),
    bulkMove: (ids, folderId) => inv('prompts_bulk_move', { ids, folderId }),
    bulkDelete: (ids) => inv('prompts_bulk_delete', { ids }),
    copyBody: (id) => inv('prompts_copy_body', { id }),
  },
  folders: {
    list: () => inv('folders_list'),
    create: (name) => inv('folders_create', { name }),
    rename: (id, name) => inv('folders_rename', { id, name }),
    setLocked: (id, locked, password) =>
      inv('folders_set_locked', { id, locked, password }),
    delete: (id, mode) => inv('folders_delete', { id, mode }),
    export: (id) => inv('folders_export', { id }),
  },
  smartFolders: {
    list: () => inv('smart_folders_list'),
    create: (name, filter) => inv('smart_folders_create', { name, filter }),
    delete: (id) => inv('smart_folders_delete', { id }),
  },
  tags: {
    list: () => inv('tags_list'),
    rename: (oldName, newName) => inv('tags_rename', { oldName, newName }),
    delete: (name) => inv('tags_delete', { name }),
  },
  data: {
    exportAll: () => inv('data_export_all'),
    exportPrompts: (ids) => inv('data_export_prompts', { ids }),
    import: (payload, mode) => inv('data_import', { payload, mode }),
    exportToFile: (payload) => inv('data_export_to_file', { payload }),
    importFromFile: () => inv('data_import_from_file'),
  },
  settings: {
    get: () => inv('settings_get'),
    setEmbedding: (config, apiKey) =>
      inv('settings_set_embedding', {
        config: {
          provider: config.provider === 'openai-compatible' ? 'openai-compatible' : 'local-minilm',
          baseUrl: config.baseUrl,
          model: config.model,
          dimensions: config.dimensions ?? 384,
        },
        apiKey: apiKey ?? null,
      }),
    clearEmbeddingApiKey: () => inv('settings_clear_embedding_api_key'),
    setRememberLock: (remember) => inv('settings_set_remember_lock', { remember }),
  },
  app: { version: () => inv('app_version') },
};

console.info('[PromptSack] Tauri bridge ready');
