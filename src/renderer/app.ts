/**
 * PromptSack renderer — three-pane catalog UI controller.
 */

import type {
  Prompt,
  Folder,
  SmartFolder,
  TagInfo,
  ListFilter,
  SortMode,
  SearchHit,
  ExportPayload,
} from '../shared/types.js';
import { buildPromptSavePatch } from '../shared/save-policy.js';
import type { PromptSackApi } from '../shared/api.js';

declare global {
  interface Window {
    promptsack: PromptSackApi;
  }
}

const api = () => window.promptsack;

type State = {
  filter: ListFilter;
  sort: SortMode;
  query: string;
  prompts: Prompt[];
  folders: Folder[];
  smartFolders: SmartFolder[];
  tags: TagInfo[];
  selectedId: string | null;
  selectMode: boolean;
  selectedIds: Set<string>;
  draftTags: string[];
  favorite: boolean;
  dirty: boolean;
  searching: boolean;
  /** True when selected prompt body/notes are ciphertext placeholders. */
  contentEncrypted: boolean;
};

const state: State = {
  filter: { kind: 'all' },
  sort: 'newest',
  query: '',
  prompts: [],
  folders: [],
  smartFolders: [],
  tags: [],
  selectedId: null,
  selectMode: false,
  selectedIds: new Set(),
  draftTags: [],
  favorite: false,
  dirty: false,
  searching: false,
  contentEncrypted: false,
};

const $ = <T extends HTMLElement>(sel: string) => document.querySelector(sel) as T;

function toast(msg: string, ms = 2200): void {
  const el = $('#toast');
  el.textContent = msg;
  el.hidden = false;
  window.setTimeout(() => {
    el.hidden = true;
  }, ms);
}

function showModal(
  title: string,
  bodyHtml: string,
  actions: { label: string; className?: string; onClick: () => void | Promise<void> }[],
  opts?: { wide?: boolean },
): void {
  const root = $('#modal-root');
  const modal = root.querySelector('.modal') as HTMLElement | null;
  modal?.classList.toggle('modal-wide', !!opts?.wide);
  $('#modal-title').textContent = title;
  $('#modal-body').innerHTML = bodyHtml;
  const foot = $('#modal-foot');
  foot.innerHTML = '';
  for (const a of actions) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = a.className ?? 'btn btn-ghost';
    btn.textContent = a.label;
    btn.addEventListener('click', async () => {
      await a.onClick();
    });
    foot.appendChild(btn);
  }
  root.hidden = false;
  root.querySelectorAll('[data-close]').forEach((n) => {
    n.addEventListener('click', () => {
      root.hidden = true;
    }, { once: true });
  });
}

function closeModal(): void {
  $('#modal-root').hidden = true;
}

/** In-app confirm — never use window.confirm (native chrome is ugly). */
function confirmDialog(opts: {
  title: string;
  message: string;
  detail?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}): Promise<boolean> {
  return new Promise((resolve) => {
    const detail = opts.detail
      ? `<p class="confirm-detail">${escapeHtml(opts.detail)}</p>`
      : '';
    showModal(
      opts.title,
      `<div class="confirm-body ${opts.danger ? 'confirm-danger' : ''}">
        <p class="confirm-message">${escapeHtml(opts.message)}</p>
        ${detail}
      </div>`,
      [
        {
          label: opts.cancelLabel ?? 'Cancel',
          className: 'btn btn-ghost',
          onClick: () => {
            closeModal();
            resolve(false);
          },
        },
        {
          label: opts.confirmLabel ?? 'Confirm',
          className: opts.danger ? 'btn btn-danger' : 'btn btn-primary',
          onClick: () => {
            closeModal();
            resolve(true);
          },
        },
      ],
    );
  });
}

/** In-app text prompt — never use window.prompt. */
function promptDialog(opts: {
  title: string;
  message?: string;
  label?: string;
  value?: string;
  placeholder?: string;
  confirmLabel?: string;
}): Promise<string | null> {
  return new Promise((resolve) => {
    const msg = opts.message ? `<p class="settings-lead">${escapeHtml(opts.message)}</p>` : '';
    showModal(
      opts.title,
      `${msg}
       <section class="settings-section">
         <label class="field">
           <span>${escapeHtml(opts.label ?? 'Value')}</span>
           <input id="prompt-dialog-input" type="text" value="${escapeHtml(opts.value ?? '')}" placeholder="${escapeHtml(opts.placeholder ?? '')}" />
         </label>
       </section>`,
      [
        {
          label: 'Cancel',
          className: 'btn btn-ghost',
          onClick: () => {
            closeModal();
            resolve(null);
          },
        },
        {
          label: opts.confirmLabel ?? 'OK',
          className: 'btn btn-primary',
          onClick: () => {
            const v = (document.getElementById('prompt-dialog-input') as HTMLInputElement).value;
            closeModal();
            resolve(v);
          },
        },
      ],
    );
    queueMicrotask(() => {
      const input = document.getElementById('prompt-dialog-input') as HTMLInputElement | null;
      input?.focus();
      input?.select();
    });
  });
}

// ── Bootstrap ───────────────────────────────────────────────────────────

async function boot(): Promise<void> {
  const status = await api().vault.status();
  $('#gate').hidden = false;
  $('#shell').hidden = true;
  $('#gate-hint').textContent = status.exists
    ? 'Enter your vault passphrase to unlock the encrypted database.'
    : 'Create a new encrypted vault. Choose a strong passphrase — it cannot be recovered.';
  $('#gate-submit').textContent = status.exists ? 'Unlock vault' : 'Create vault';

  $('#gate-form').addEventListener('submit', async (e) => {
    e.preventDefault();
    const pass = ($('#gate-pass') as HTMLInputElement).value;
    const err = $('#gate-error');
    err.hidden = true;
    try {
      await api().vault.open(pass);
      await enterShell();
    } catch (ex) {
      err.hidden = false;
      err.textContent = ex instanceof Error ? ex.message : String(ex);
    }
  });
}

async function enterShell(): Promise<void> {
  $('#gate').hidden = true;
  $('#shell').hidden = false;
  wireShell();
  await refreshAll();
}

function wireShell(): void {
  // Search
  const search = $('#search-input') as HTMLInputElement;
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  search.addEventListener('input', () => {
    state.query = search.value;
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => void runSearch(), 180);
  });

  document.addEventListener('keydown', (e) => {
    if (e.key === '/' && document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA') {
      e.preventDefault();
      search.focus();
    }
  });

  // Nav filters
  document.querySelectorAll<HTMLElement>('.nav-item[data-filter]').forEach((el) => {
    el.addEventListener('click', () => {
      state.filter = JSON.parse(el.dataset.filter || '{"kind":"all"}') as ListFilter;
      state.query = '';
      search.value = '';
      document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
      el.classList.add('active');
      void refreshPrompts();
    });
  });

  ($('#sort-select') as HTMLSelectElement).addEventListener('change', (e) => {
    state.sort = (e.target as HTMLSelectElement).value as SortMode;
    void refreshPrompts();
  });

  $('#btn-new-prompt').addEventListener('click', () => void createPrompt());
  $('#btn-empty-new').addEventListener('click', () => void createPrompt());
  $('#btn-new-folder').addEventListener('click', () => void promptNewFolder());
  $('#btn-new-smart')?.addEventListener('click', () => void promptNewSmartFolder());
  $('#btn-manage-tags').addEventListener('click', () => void openTagManager());
  $('#btn-settings').addEventListener('click', () => void openSettings());

  const exitSelectMode = () => {
    state.selectMode = false;
    state.selectedIds.clear();
    updateSelectChrome();
    renderList();
  };

  const enterSelectMode = () => {
    state.selectMode = true;
    state.selectedIds.clear();
    updateSelectChrome();
    renderList();
  };

  $('#btn-select-mode').addEventListener('click', () => {
    if (state.selectMode) exitSelectMode();
    else enterSelectMode();
  });
  $('#btn-select-done').addEventListener('click', () => exitSelectMode());

  $('#btn-bulk-delete').addEventListener('click', async () => {
    if (!state.selectedIds.size) return;
    const n = state.selectedIds.size;
    const ok = await confirmDialog({
      title: 'Delete prompts',
      message: `Delete ${n} selected prompt${n === 1 ? '' : 's'}?`,
      detail: 'This removes the markdown files and search index entries. This cannot be undone.',
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    await api().prompts.bulkDelete([...state.selectedIds]);
    state.selectedIds.clear();
    updateSelectChrome();
    toast('Deleted');
    await refreshAll();
  });

  $('#btn-bulk-export').addEventListener('click', async () => {
    if (!state.selectedIds.size) return;
    const payload = await api().data.exportPrompts([...state.selectedIds]);
    await api().data.exportToFile(payload);
    toast('Exported selection');
  });

  $('#btn-bulk-move').addEventListener('click', () => {
    if (!state.selectedIds.size) return;
    void openBulkMove();
  });

  // Editor
  const body = $('#ed-body') as HTMLTextAreaElement;
  body.addEventListener('input', () => {
    $('#char-count').textContent = `${body.value.length} characters`;
    state.dirty = true;
  });
  ($('#ed-title') as HTMLInputElement).addEventListener('input', () => {
    state.dirty = true;
  });
  ($('#ed-notes') as HTMLTextAreaElement).addEventListener('input', () => {
    state.dirty = true;
  });

  $('#editor-form').addEventListener('submit', (e) => {
    e.preventDefault();
    void saveEditor();
  });

  $('#btn-delete').addEventListener('click', async () => {
    if (!state.selectedId) return;
    const title =
      ($('#ed-title') as HTMLInputElement).value.trim() ||
      state.prompts.find((p) => p.id === state.selectedId)?.title ||
      'this prompt';
    const ok = await confirmDialog({
      title: 'Delete prompt',
      message: `Delete “${title}”?`,
      detail: 'The markdown file and its search index entry will be removed permanently.',
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    await api().prompts.delete(state.selectedId);
    state.selectedId = null;
    toast('Prompt deleted');
    await refreshAll();
    showEditorEmpty();
  });

  $('#btn-favorite').addEventListener('click', () => {
    state.favorite = !state.favorite;
    syncFavoriteButton();
    state.dirty = true;
  });

  $('#ed-tag-input').addEventListener('keydown', (e) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      const input = e.target as HTMLInputElement;
      const t = input.value.trim().replace(/,$/, '');
      if (t && !state.draftTags.includes(t)) {
        state.draftTags.push(t);
        renderTagChips();
        state.dirty = true;
      }
      input.value = '';
    } else if (e.key === 'Backspace' && !(e.target as HTMLInputElement).value && state.draftTags.length) {
      state.draftTags.pop();
      renderTagChips();
      state.dirty = true;
    }
  });

  $('#btn-fullscreen-body').addEventListener('click', () => openFullscreen('body'));
  $('#btn-fullscreen-notes').addEventListener('click', () => openFullscreen('notes'));

  $('#btn-unlock-prompt').addEventListener('click', () => void unlockSessionDialog());

  const list = $('#prompt-list');
  list.addEventListener('scroll', () => {
    $('#btn-back-top').hidden = list.scrollTop < 200;
  });
  $('#btn-back-top').addEventListener('click', () => {
    list.scrollTo({ top: 0, behavior: 'smooth' });
  });
}

async function refreshAll(): Promise<void> {
  state.folders = await api().folders.list();
  state.smartFolders = (await api().smartFolders.list()) ?? [];
  state.tags = await api().tags.list();
  await refreshPrompts();
  renderSidebar();
}

async function refreshPrompts(): Promise<void> {
  if (state.query.trim()) {
    await runSearch();
    return;
  }
  state.prompts = await api().prompts.list(state.filter, state.sort);
  updateCounts();
  renderList();
}

async function runSearch(): Promise<void> {
  const q = state.query.trim();
  if (!q) {
    state.searching = false;
    await refreshPrompts();
    return;
  }
  state.searching = true;
  const hits: SearchHit[] = await api().prompts.search(q);
  state.prompts = hits.map((h) => h.prompt);
  renderList(hits);
}

function updateCounts(): void {
  // Approximate from current all-list via a lightweight approach:
  void (async () => {
    const all = await api().prompts.list({ kind: 'all' }, 'newest');
    $('#count-all').textContent = String(all.length);
    $('#count-fav').textContent = String(all.filter((p) => p.favorite).length);
    $('#count-locked').textContent = String(all.filter((p) => p.locked).length);
  })();
}

function renderSidebar(): void {
  const fl = $('#folder-list');
  fl.innerHTML = '';
  for (const f of state.folders) {
    const wrap = document.createElement('div');
    wrap.className = 'folder-item';
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'nav-item';
    if (state.filter.kind === 'folder' && state.filter.folderId === f.id) btn.classList.add('active');
    btn.innerHTML = `<span>${escapeHtml(f.name)}${f.locked ? ' 🔒' : ''}</span>`;
    btn.addEventListener('click', () => {
      state.filter = { kind: 'folder', folderId: f.id };
      state.query = '';
      ($('#search-input') as HTMLInputElement).value = '';
      document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
      btn.classList.add('active');
      void refreshPrompts();
    });
    // DnD target
    btn.addEventListener('dragover', (e) => {
      e.preventDefault();
      btn.classList.add('drop-target');
    });
    btn.addEventListener('dragleave', () => btn.classList.remove('drop-target'));
    btn.addEventListener('drop', async (e) => {
      e.preventDefault();
      btn.classList.remove('drop-target');
      const id = e.dataTransfer?.getData('text/prompt-id');
      if (!id) return;
      await api().prompts.update(id, { folderId: f.id });
      toast(`Moved to ${f.name}`);
      await refreshAll();
    });

    const menu = document.createElement('button');
    menu.type = 'button';
    menu.className = 'icon-btn folder-menu';
    menu.textContent = '⋯';
    menu.title = 'Folder actions';
    menu.addEventListener('click', (e) => {
      e.stopPropagation();
      openFolderMenu(f);
    });

    wrap.appendChild(btn);
    wrap.appendChild(menu);
    fl.appendChild(wrap);
  }

  const sfl = document.getElementById('smart-folder-list');
  if (sfl) {
    sfl.innerHTML = '';
    for (const sf of state.smartFolders) {
      const wrap = document.createElement('div');
      wrap.className = 'folder-item';
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'nav-item';
      if (state.filter.kind === 'smart' && state.filter.smartFolderId === sf.id) {
        btn.classList.add('active');
      }
      btn.innerHTML = `<span>✦ ${escapeHtml(sf.name)}</span>`;
      btn.addEventListener('click', () => {
        state.filter = { kind: 'smart', smartFolderId: sf.id };
        state.query = '';
        ($('#search-input') as HTMLInputElement).value = '';
        document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
        btn.classList.add('active');
        void refreshPrompts();
      });
      const menu = document.createElement('button');
      menu.type = 'button';
      menu.className = 'icon-btn folder-menu';
      menu.textContent = '⋯';
      menu.title = 'Smart folder actions';
      menu.addEventListener('click', (e) => {
        e.stopPropagation();
        void openSmartFolderMenu(sf);
      });
      wrap.appendChild(btn);
      wrap.appendChild(menu);
      sfl.appendChild(wrap);
    }
    if (!state.smartFolders.length) {
      const empty = document.createElement('p');
      empty.className = 'hint';
      empty.style.cssText = 'padding:0.25rem 0.5rem;margin:0;font-size:0.75rem';
      empty.textContent = 'Save a filter with +';
      sfl.appendChild(empty);
    }
  }

  const tl = $('#tag-list');
  tl.innerHTML = '';
  for (const t of state.tags) {
    const pill = document.createElement('button');
    pill.type = 'button';
    pill.className = 'tag-pill';
    if (state.filter.kind === 'tag' && state.filter.tag === t.name) pill.classList.add('active');
    pill.innerHTML = `${escapeHtml(t.name)}<span class="n">${t.count}</span>`;
    pill.addEventListener('click', () => {
      state.filter = { kind: 'tag', tag: t.name };
      state.query = '';
      ($('#search-input') as HTMLInputElement).value = '';
      document.querySelectorAll('.nav-item').forEach((n) => n.classList.remove('active'));
      void refreshPrompts();
      renderSidebar();
    });
    tl.appendChild(pill);
  }
}

function updateSelectChrome(): void {
  const normal = $('#list-toolbar-normal');
  const select = $('#list-toolbar-select');
  const list = $('#prompt-list');
  if (state.selectMode) {
    normal.hidden = true;
    select.hidden = false;
    list.classList.add('selecting');
  } else {
    normal.hidden = false;
    select.hidden = true;
    list.classList.remove('selecting');
  }
  const n = state.selectedIds.size;
  $('#bulk-count').textContent = String(n);
  const disabled = n === 0;
  ($('#btn-bulk-move') as HTMLButtonElement).disabled = disabled;
  ($('#btn-bulk-export') as HTMLButtonElement).disabled = disabled;
  ($('#btn-bulk-delete') as HTMLButtonElement).disabled = disabled;
}

function renderList(hits?: SearchHit[]): void {
  const root = $('#prompt-list');
  root.innerHTML = '';
  updateSelectChrome();

  if (!state.prompts.length) {
    const empty = document.createElement('div');
    empty.className = 'list-empty';
    if (state.selectMode) {
      empty.innerHTML =
        '<strong>Nothing to select</strong><span>This view has no prompts yet. Create one or switch folders.</span>';
    } else if (state.query) {
      empty.innerHTML =
        '<strong>No matches</strong><span>Try a different phrase — search is semantic, not just keywords.</span>';
    } else {
      empty.innerHTML =
        '<strong>No prompts here yet</strong><span>Create a prompt or drop one onto this folder.</span>';
    }
    root.appendChild(empty);
    return;
  }

  const scoreMap = new Map(hits?.map((h) => [h.prompt.id, h]) ?? []);

  for (const p of state.prompts) {
    const row = document.createElement('div');
    row.className = 'prompt-row';
    row.draggable = !state.selectMode;
    row.role = 'listitem';
    if (p.id === state.selectedId && !state.selectMode) row.classList.add('active');
    if (state.selectMode) row.classList.add('selecting');
    if (state.selectedIds.has(p.id)) row.classList.add('checked');

    row.addEventListener('dragstart', (e) => {
      e.dataTransfer?.setData('text/prompt-id', p.id);
    });

    if (state.selectMode) {
      const check = document.createElement('input');
      check.type = 'checkbox';
      check.className = 'prompt-check';
      check.checked = state.selectedIds.has(p.id);
      check.setAttribute('aria-label', `Select ${p.title}`);
      check.addEventListener('click', (e) => e.stopPropagation());
      check.addEventListener('change', () => {
        if (check.checked) state.selectedIds.add(p.id);
        else state.selectedIds.delete(p.id);
        row.classList.toggle('checked', check.checked);
        updateSelectChrome();
      });
      row.appendChild(check);
      row.addEventListener('click', (e) => {
        if ((e.target as HTMLElement).closest('input,button')) return;
        check.checked = !check.checked;
        check.dispatchEvent(new Event('change'));
      });
    } else {
      const spacer = document.createElement('div');
      spacer.style.width = '0';
      row.appendChild(spacer);
    }

    const mid = document.createElement('div');
    const title = document.createElement('div');
    title.className = 'title';
    title.textContent = p.title + (p.locked ? ' 🔒' : '');
    if (p.favorite) {
      const star = document.createElement('span');
      star.className = 'fav-star';
      star.textContent = ' ★';
      star.title = 'Favorite';
      title.appendChild(star);
    }
    mid.appendChild(title);
    const tags = document.createElement('div');
    tags.className = 'tags';
    for (const t of p.tags.slice(0, 6)) {
      const tag = document.createElement('span');
      tag.className = 'tag';
      tag.textContent = t;
      tags.appendChild(tag);
    }
    const hit = scoreMap.get(p.id);
    if (hit && state.query) {
      const badge = document.createElement('span');
      badge.className = 'tag';
      badge.style.borderColor = 'rgba(34,211,238,0.4)';
      badge.textContent = hit.source;
      tags.appendChild(badge);
    }
    mid.appendChild(tags);
    row.appendChild(mid);

    const actions = document.createElement('div');
    actions.className = 'actions';
    const copyBtn = document.createElement('button');
    copyBtn.type = 'button';
    copyBtn.className = 'icon-btn';
    copyBtn.title = 'Copy body';
    copyBtn.textContent = '⧉';
    copyBtn.addEventListener('click', async (e) => {
      e.stopPropagation();
      try {
        await api().prompts.copyBody(p.id);
        toast('Copied to clipboard');
      } catch (err) {
        toast(err instanceof Error ? err.message : 'Copy failed');
      }
    });
    actions.appendChild(copyBtn);
    row.appendChild(actions);

    if (!state.selectMode) {
      row.addEventListener('click', () => void selectPrompt(p.id));
    }
    root.appendChild(row);
  }
}

async function selectPrompt(id: string): Promise<void> {
  const p = await api().prompts.get(id);
  if (!p) return;
  state.selectedId = id;
  state.dirty = false;
  state.favorite = p.favorite;
  state.draftTags = [...p.tags];
  state.contentEncrypted = !!p.contentEncrypted;
  renderList();

  $('#editor-empty').hidden = true;
  $('#editor-form').hidden = false;

  ($('#ed-title') as HTMLInputElement).value = p.title;
  ($('#ed-body') as HTMLTextAreaElement).value = p.contentEncrypted ? '' : p.body;
  ($('#ed-notes') as HTMLTextAreaElement).value = p.contentEncrypted ? '' : p.notes;
  ($('#ed-lock') as HTMLInputElement).checked = p.locked;
  $('#char-count').textContent = `${(p.contentEncrypted ? 0 : p.body.length)} characters`;
  syncFavoriteButton();
  $('#editor-locked-banner').hidden = !p.contentEncrypted;

  const folderSel = $('#ed-folder') as HTMLSelectElement;
  folderSel.innerHTML = `<option value="">— Unfiled —</option>`;
  for (const f of state.folders) {
    const opt = document.createElement('option');
    opt.value = f.id;
    opt.textContent = f.name;
    if (f.id === p.folderId) opt.selected = true;
    folderSel.appendChild(opt);
  }
  renderTagChips();
}

function showEditorEmpty(): void {
  $('#editor-empty').hidden = false;
  $('#editor-form').hidden = true;
}

function syncFavoriteButton(): void {
  const btn = $('#btn-favorite');
  const on = state.favorite;
  btn.textContent = on ? '★' : '☆';
  btn.classList.toggle('is-favorite', on);
  btn.setAttribute('aria-pressed', on ? 'true' : 'false');
  btn.title = on ? 'Favorited — click to remove' : 'Mark as favorite';
}

function renderTagChips(): void {
  const root = $('#ed-tags');
  root.innerHTML = '';
  for (const t of state.draftTags) {
    const chip = document.createElement('span');
    chip.className = 'chip';
    chip.innerHTML = `${escapeHtml(t)} <button type="button" aria-label="Remove">×</button>`;
    chip.querySelector('button')!.addEventListener('click', () => {
      state.draftTags = state.draftTags.filter((x) => x !== t);
      renderTagChips();
      state.dirty = true;
    });
    root.appendChild(chip);
  }
}

async function saveEditor(): Promise<void> {
  if (!state.selectedId) return;

  // Keep contentEncrypted flag in sync with the locked banner / loaded state.
  state.contentEncrypted =
    state.contentEncrypted || !$('#editor-locked-banner').hidden;

  if (state.contentEncrypted) {
    const session = await api().session.status();
    if (!session.unlocked) {
      const ok = await unlockSessionDialog();
      if (!ok) return;
    }
    // Reload plaintext into the form — never save empty placeholders.
    await selectPrompt(state.selectedId);
    if (state.contentEncrypted) {
      toast('Unlock the session to edit locked content');
      return;
    }
    toast('Unlocked — review content, then save');
    return;
  }

  const title = ($('#ed-title') as HTMLInputElement).value;
  const body = ($('#ed-body') as HTMLTextAreaElement).value;
  const notes = ($('#ed-notes') as HTMLTextAreaElement).value;
  const locked = ($('#ed-lock') as HTMLInputElement).checked;
  const folderRaw = ($('#ed-folder') as HTMLSelectElement).value;
  const folderId = folderRaw || null;

  if (locked) {
    const session = await api().session.status();
    if (!session.unlocked) {
      const ok = await unlockSessionDialog();
      if (!ok) return;
    }
  }

  const policy = buildPromptSavePatch({
    title,
    body,
    notes,
    tags: state.draftTags,
    folderId,
    favorite: state.favorite,
    locked,
    contentEncrypted: state.contentEncrypted,
    sessionUnlocked: (await api().session.status()).unlocked,
  });
  if (!policy.ok) {
    toast(policy.reason);
    return;
  }

  try {
    await api().prompts.update(state.selectedId, policy.patch);
    state.dirty = false;
    toast('Saved');
    await refreshAll();
    await selectPrompt(state.selectedId);
  } catch (err) {
    toast(err instanceof Error ? err.message : 'Save failed');
  }
}

async function createPrompt(): Promise<void> {
  const folderId = state.filter.kind === 'folder' ? state.filter.folderId : null;
  const p = await api().prompts.create({
    title: 'Untitled prompt',
    body: '',
    notes: '',
    tags: [],
    folderId,
  });
  await refreshAll();
  await selectPrompt(p.id);
  ($('#ed-title') as HTMLInputElement).focus();
  ($('#ed-title') as HTMLInputElement).select();
}

function promptNewFolder(): void {
  showModal(
    'New folder',
    `<section class="settings-section">
      <h4 class="settings-section-title">Details</h4>
      <label class="field">
        <span>Name</span>
        <input id="modal-folder-name" type="text" placeholder="e.g. Research, Client work…" maxlength="80" />
      </label>
      <p class="settings-meta">Folders group prompts in the sidebar. You can rename or lock them later.</p>
    </section>`,
    [
      { label: 'Cancel', className: 'btn btn-ghost', onClick: () => closeModal() },
      {
        label: 'Create folder',
        className: 'btn btn-primary',
        onClick: async () => {
          const name = (document.getElementById('modal-folder-name') as HTMLInputElement).value;
          if (!name.trim()) {
            toast('Enter a folder name');
            return;
          }
          await api().folders.create(name);
          closeModal();
          toast('Folder created');
          await refreshAll();
        },
      },
    ],
  );
  queueMicrotask(() => (document.getElementById('modal-folder-name') as HTMLInputElement)?.focus());
}

function filterLabel(f: ListFilter): string {
  switch (f.kind) {
    case 'all':
      return 'All prompts';
    case 'favorites':
      return 'Favorites';
    case 'locked':
      return 'Locked';
    case 'folder': {
      const name = state.folders.find((x) => x.id === f.folderId)?.name;
      return name ? `Folder: ${name}` : 'Folder';
    }
    case 'tag':
      return `Tag: ${f.tag}`;
    case 'smart':
      return 'Smart folder';
    default:
      return 'Current view';
  }
}

function promptNewSmartFolder(): void {
  // Don't nest smart-on-smart
  const baseFilter: ListFilter =
    state.filter.kind === 'smart' ? { kind: 'all' } : { ...state.filter };
  if (baseFilter.kind === 'all' && !state.query.trim()) {
    toast('Open Favorites, Locked, a folder, or a tag first — then save it');
    return;
  }
  showModal(
    'New smart folder',
    `<p class="settings-lead">Saves the current filter so you can jump back in one click.</p>
     <section class="settings-section">
       <h4 class="settings-section-title">Details</h4>
       <label class="field">
         <span>Name</span>
         <input id="modal-smart-name" type="text" placeholder="e.g. Ops tags, Favorites weekly…" maxlength="80" />
       </label>
       <p class="settings-meta">Filter · ${escapeHtml(filterLabel(baseFilter))}</p>
     </section>`,
    [
      { label: 'Cancel', className: 'btn btn-ghost', onClick: () => closeModal() },
      {
        label: 'Save smart folder',
        className: 'btn btn-primary',
        onClick: async () => {
          const name = (document.getElementById('modal-smart-name') as HTMLInputElement).value;
          if (!name.trim()) {
            toast('Enter a name');
            return;
          }
          const sf = await api().smartFolders.create(name.trim(), baseFilter);
          closeModal();
          state.filter = { kind: 'smart', smartFolderId: sf.id };
          toast('Smart folder saved');
          await refreshAll();
        },
      },
    ],
  );
  queueMicrotask(() => (document.getElementById('modal-smart-name') as HTMLInputElement)?.focus());
}

async function openSmartFolderMenu(sf: SmartFolder): Promise<void> {
  showModal(
    sf.name,
    `<p class="settings-lead">Smart folder · ${escapeHtml(filterLabel(sf.filter))}</p>
     <section class="settings-section settings-danger">
       <h4 class="settings-section-title">Danger zone</h4>
       <div class="menu-list">
         <button type="button" class="menu-item danger" id="smart-delete">
           <div class="menu-item-main">
             <strong>Delete smart folder</strong>
             <span>Removes the saved filter only — prompts stay intact</span>
           </div>
           <span class="menu-item-chevron">›</span>
         </button>
       </div>
     </section>`,
    [{ label: 'Close', className: 'btn btn-ghost', onClick: () => closeModal() }],
  );
  document.getElementById('smart-delete')?.addEventListener('click', async () => {
    const ok = await confirmDialog({
      title: 'Delete smart folder',
      message: `Delete “${sf.name}”?`,
      detail: 'Only the saved filter is removed. Prompts are not deleted.',
      confirmLabel: 'Delete',
      danger: true,
    });
    if (!ok) return;
    await api().smartFolders.delete(sf.id);
    if (state.filter.kind === 'smart' && state.filter.smartFolderId === sf.id) {
      state.filter = { kind: 'all' };
    }
    closeModal();
    toast('Smart folder deleted');
    await refreshAll();
  });
}

function openFolderMenu(f: Folder): void {
  const body = `
    <p class="settings-lead">${f.locked ? 'This folder is locked.' : 'Organize, protect, or export this folder.'}</p>
    <div id="folder-menu-root">
      <section class="settings-section">
        <h4 class="settings-section-title">Actions</h4>
        <div class="menu-list">
          <button type="button" class="menu-item" data-act="rename">
            <div class="menu-item-main">
              <strong>Rename</strong>
              <span>Change how this folder appears in the sidebar</span>
            </div>
            <span class="menu-item-chevron">›</span>
          </button>
          <button type="button" class="menu-item" data-act="lock">
            <div class="menu-item-main">
              <strong>${f.locked ? 'Unlock folder' : 'Lock folder'}</strong>
              <span>${f.locked ? 'Allow access without a session password' : 'Require session unlock for prompts in this folder'}</span>
            </div>
            <span class="menu-item-chevron">›</span>
          </button>
          <button type="button" class="menu-item" data-act="export">
            <div class="menu-item-main">
              <strong>Export folder</strong>
              <span>Download prompts in this folder as JSON</span>
            </div>
            <span class="menu-item-chevron">›</span>
          </button>
        </div>
      </section>
      <section class="settings-section settings-danger">
        <h4 class="settings-section-title">Danger zone</h4>
        <div class="menu-list">
          <button type="button" class="menu-item danger" data-act="delete-move">
            <div class="menu-item-main">
              <strong>Delete folder</strong>
              <span>Remove the folder and unfile its prompts</span>
            </div>
            <span class="menu-item-chevron">›</span>
          </button>
          <button type="button" class="menu-item danger" data-act="delete-all">
            <div class="menu-item-main">
              <strong>Delete folder &amp; prompts</strong>
              <span>Permanently delete the folder and every prompt inside</span>
            </div>
            <span class="menu-item-chevron">›</span>
          </button>
        </div>
      </section>
    </div>
    <div id="folder-menu-panel" hidden></div>`;

  showModal(f.name, body, [
    { label: 'Close', className: 'btn btn-ghost', onClick: () => closeModal() },
  ]);

  const root = document.getElementById('folder-menu-root')!;
  const panel = document.getElementById('folder-menu-panel')!;

  const showPanel = (html: string) => {
    root.hidden = true;
    panel.hidden = false;
    panel.innerHTML = html;
  };
  const showMenu = () => {
    panel.hidden = true;
    panel.innerHTML = '';
    root.hidden = false;
  };

  root.querySelectorAll<HTMLElement>('[data-act]').forEach((el) => {
    el.addEventListener('click', async () => {
      const act = el.dataset.act;
      if (act === 'rename') {
        showPanel(`
          <div class="inline-panel">
            <label class="field">
              <span>New name</span>
              <input id="folder-rename-input" type="text" value="${escapeHtml(f.name)}" maxlength="80" />
            </label>
            <div class="inline-panel-actions">
              <button type="button" class="btn btn-ghost" id="folder-rename-cancel">Back</button>
              <button type="button" class="btn btn-primary" id="folder-rename-save">Rename</button>
            </div>
          </div>`);
        const input = document.getElementById('folder-rename-input') as HTMLInputElement;
        input.focus();
        input.select();
        document.getElementById('folder-rename-cancel')!.onclick = () => showMenu();
        document.getElementById('folder-rename-save')!.onclick = async () => {
          const name = input.value.trim();
          if (!name) {
            toast('Enter a name');
            return;
          }
          await api().folders.rename(f.id, name);
          closeModal();
          toast('Folder renamed');
          await refreshAll();
        };
        return;
      }
      if (act === 'lock') {
        if (f.locked) {
          await api().folders.setLocked(f.id, false);
          closeModal();
          toast('Folder unlocked');
          await refreshAll();
          return;
        }
        showPanel(`
          <div class="inline-panel">
            <label class="field">
              <span>Lock password</span>
              <input id="folder-lock-pass" type="password" placeholder="Session unlock password" autocomplete="new-password" />
            </label>
            <p class="settings-meta">Uses the same session password as locked prompts. Saved to OS keychain when remember is enabled.</p>
            <div class="inline-panel-actions">
              <button type="button" class="btn btn-ghost" id="folder-lock-cancel">Back</button>
              <button type="button" class="btn btn-primary" id="folder-lock-save">Lock folder</button>
            </div>
          </div>`);
        const pw = document.getElementById('folder-lock-pass') as HTMLInputElement;
        pw.focus();
        document.getElementById('folder-lock-cancel')!.onclick = () => showMenu();
        document.getElementById('folder-lock-save')!.onclick = async () => {
          if (!pw.value) {
            toast('Enter a password');
            return;
          }
          await api().folders.setLocked(f.id, true, pw.value);
          closeModal();
          toast('Folder locked');
          await refreshAll();
        };
        return;
      }
      if (act === 'export') {
        const payload = await api().folders.export(f.id);
        await api().data.exportToFile(payload);
        closeModal();
        toast('Folder exported');
        return;
      }
      if (act === 'delete-move') {
        const ok = await confirmDialog({
          title: 'Delete folder',
          message: `Delete folder “${f.name}”?`,
          detail: 'Prompts stay in your library and become unfiled.',
          confirmLabel: 'Delete folder',
          danger: true,
        });
        if (!ok) return;
        await api().folders.delete(f.id, 'move');
        closeModal();
        state.filter = { kind: 'all' };
        toast('Folder deleted');
        await refreshAll();
        return;
      }
      if (act === 'delete-all') {
        const ok = await confirmDialog({
          title: 'Delete folder & prompts',
          message: `Delete “${f.name}” and every prompt inside?`,
          detail: 'All markdown files in this folder will be permanently removed.',
          confirmLabel: 'Delete everything',
          danger: true,
        });
        if (!ok) return;
        await api().folders.delete(f.id, 'delete');
        closeModal();
        state.filter = { kind: 'all' };
        toast('Folder and prompts deleted');
        await refreshAll();
      }
    });
  });
}

function openTagManager(): void {
  const rows = state.tags
    .map(
      (t) => `
      <div class="tag-manage-row" data-tag="${escapeHtml(t.name)}">
        <div class="tag-manage-name">
          <span>${escapeHtml(t.name)}</span>
          <span class="count">${t.count}</span>
        </div>
        <div class="tag-manage-actions">
          <button type="button" class="btn btn-ghost btn-sm" data-ren="${escapeHtml(t.name)}">Rename</button>
          <button type="button" class="btn btn-danger btn-sm" data-del="${escapeHtml(t.name)}">Delete</button>
        </div>
      </div>`,
    )
    .join('');

  showModal(
    'Manage tags',
    state.tags.length
      ? `<p class="settings-lead">Rename or delete tags across every prompt.</p>
         <div class="tag-manage-list">${rows}</div>
         <div id="tag-inline-panel" hidden></div>`
      : `<p class="modal-empty">No tags yet — add tags when editing a prompt.</p>`,
    [{ label: 'Close', className: 'btn btn-ghost', onClick: () => closeModal() }],
    { wide: true },
  );

  const list = $('#modal-body').querySelector('.tag-manage-list') as HTMLElement | null;
  const panel = document.getElementById('tag-inline-panel');

  $('#modal-body').querySelectorAll('[data-ren]').forEach((btn) => {
    btn.addEventListener('click', () => {
      const oldName = (btn as HTMLElement).dataset.ren!;
      if (!list || !panel) {
        void (async () => {
          const next = await promptDialog({
            title: 'Rename tag',
            label: 'New name',
            value: oldName,
            confirmLabel: 'Rename',
          });
          if (next === null || !next.trim() || next.trim() === oldName) return;
          await api().tags.rename(oldName, next.trim());
          closeModal();
          await refreshAll();
          openTagManager();
        })();
        return;
      }
      list.hidden = true;
      panel.hidden = false;
      panel.innerHTML = `
        <div class="inline-panel">
          <label class="field">
            <span>Rename “${escapeHtml(oldName)}”</span>
            <input id="tag-rename-input" type="text" value="${escapeHtml(oldName)}" />
          </label>
          <div class="inline-panel-actions">
            <button type="button" class="btn btn-ghost" id="tag-rename-cancel">Back</button>
            <button type="button" class="btn btn-primary" id="tag-rename-save">Save</button>
          </div>
        </div>`;
      const input = document.getElementById('tag-rename-input') as HTMLInputElement;
      input.focus();
      input.select();
      document.getElementById('tag-rename-cancel')!.onclick = () => {
        panel.hidden = true;
        list.hidden = false;
      };
      document.getElementById('tag-rename-save')!.onclick = async () => {
        const next = input.value.trim();
        if (!next || next === oldName) return;
        await api().tags.rename(oldName, next);
        closeModal();
        toast('Tag renamed');
        await refreshAll();
        openTagManager();
      };
    });
  });

  $('#modal-body').querySelectorAll('[data-del]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const name = (btn as HTMLElement).dataset.del!;
      const ok = await confirmDialog({
        title: 'Delete tag',
        message: `Remove tag “${name}” from all prompts?`,
        detail: 'Prompts stay; only the tag association is cleared.',
        confirmLabel: 'Delete tag',
        danger: true,
      });
      if (!ok) return;
      await api().tags.delete(name);
      closeModal();
      toast('Tag deleted');
      await refreshAll();
      openTagManager();
    });
  });
}

async function openSettings(): Promise<void> {
  let settingsHtml = `
    <p class="settings-lead">
      Vault data is encrypted at rest with MongrelDB. Lock passwords and API keys live in the OS keychain — never in the database.
    </p>`;

  try {
    const s = await api().settings.get();
    settingsHtml += `
      <section class="settings-section">
        <h4 class="settings-section-title">Search & embeddings</h4>
        <label class="field">
          <span>Provider</span>
          <select id="set-emb-provider" class="select">
            <option value="local-minilm" ${s.embedding.provider === 'local-minilm' ? 'selected' : ''}>Local MiniLM (all-MiniLM-L6-v2 · 384-d)</option>
            <option value="openai-compatible" ${s.embedding.provider === 'openai-compatible' ? 'selected' : ''}>OpenAI-compatible API (384-d)</option>
          </select>
        </label>
        <div id="set-openai-fields" ${s.embedding.provider === 'openai-compatible' ? '' : 'hidden'}>
          <div class="field" style="gap:.65rem">
            <label class="field"><span>Base URL</span>
              <input id="set-emb-url" type="text" value="${escapeHtml(s.embedding.baseUrl || 'https://api.openai.com/v1')}" placeholder="https://api.openai.com/v1" />
            </label>
            <label class="field"><span>Model</span>
              <input id="set-emb-model" type="text" value="${escapeHtml(s.embedding.model || 'text-embedding-3-small')}" placeholder="text-embedding-3-small" />
            </label>
            <label class="field"><span>API key ${s.hasEmbeddingApiKey ? '(in keychain)' : ''}</span>
              <input id="set-emb-key" type="password" placeholder="${s.hasEmbeddingApiKey ? '••••••••  leave blank to keep' : 'sk-…'}" autocomplete="off" />
            </label>
          </div>
        </div>
        <p class="settings-meta">Active · ${escapeHtml(s.embedderModelId)} · keychain · ${escapeHtml(s.keychainBackend)}</p>
      </section>

      <section class="settings-section">
        <h4 class="settings-section-title">Session & privacy</h4>
        <div class="settings-toggle-row">
          <div class="settings-row-label">
            <strong>Remember lock password</strong>
            <span>Store in OS keychain for auto-unlock</span>
          </div>
          <label class="toggle">
            <span>Remember</span>
            <input id="set-remember-lock" type="checkbox" ${s.rememberLockInKeychain ? 'checked' : ''} />
            <span class="toggle-ui"></span>
          </label>
        </div>
        <div class="settings-actions-grid">
          <button type="button" class="btn btn-ghost" id="set-unlock-session">Unlock session</button>
          <button type="button" class="btn btn-ghost" id="set-lock-session">Lock session</button>
        </div>
      </section>

      <section class="settings-section">
        <h4 class="settings-section-title">Data</h4>
        <div class="settings-actions-grid">
          <button type="button" class="btn btn-ghost" id="set-export">Export vault</button>
          <button type="button" class="btn btn-ghost" id="set-import">Import…</button>
        </div>
      </section>

      <section class="settings-section settings-danger">
        <h4 class="settings-section-title">Danger zone</h4>
        <div class="settings-row">
          <div class="settings-row-label">
            <strong>Forget keychain lock</strong>
            <span>Remove saved lock password from the OS keychain</span>
          </div>
          <button type="button" class="btn btn-danger btn-sm" id="set-forget-keychain">Forget</button>
        </div>
      </section>`;
  } catch {
    settingsHtml += `<p class="settings-lead">Open a vault to configure embedding providers and session options.</p>`;
  }

  showModal(
    'Settings',
    settingsHtml,
    [
      { label: 'Close', className: 'btn btn-ghost', onClick: () => closeModal() },
      {
        label: 'Save',
        className: 'btn btn-primary',
        onClick: async () => {
          const provider = (document.getElementById('set-emb-provider') as HTMLSelectElement | null)
            ?.value as 'local-minilm' | 'openai-compatible' | undefined;
          if (!provider) {
            closeModal();
            return;
          }
          try {
            if (provider === 'local-minilm') {
              await api().settings.setEmbedding({ provider: 'local-minilm' });
            } else {
              const baseUrl = (document.getElementById('set-emb-url') as HTMLInputElement).value;
              const model = (document.getElementById('set-emb-model') as HTMLInputElement).value;
              const apiKey = (document.getElementById('set-emb-key') as HTMLInputElement).value;
              await api().settings.setEmbedding(
                { provider: 'openai-compatible', baseUrl, model, dimensions: 384 },
                apiKey || null,
              );
            }
            const remember = (document.getElementById('set-remember-lock') as HTMLInputElement)
              ?.checked;
            if (typeof remember === 'boolean') {
              await api().settings.setRememberLock(remember);
            }
            toast('Settings saved');
            closeModal();
          } catch (err) {
            toast(err instanceof Error ? err.message : 'Save failed');
          }
        },
      },
    ],
    { wide: true },
  );

  queueMicrotask(() => {
    const sel = document.getElementById('set-emb-provider') as HTMLSelectElement | null;
    const fields = document.getElementById('set-openai-fields');
    sel?.addEventListener('change', () => {
      if (!fields) return;
      fields.hidden = sel.value !== 'openai-compatible';
    });

    document.getElementById('set-export')?.addEventListener('click', async () => {
      const payload = await api().data.exportAll();
      await api().data.exportToFile(payload);
      toast('Exported');
    });
    document.getElementById('set-import')?.addEventListener('click', async () => {
      const res = await api().data.importFromFile();
      if (res.ok) {
        toast(`Imported ${res.prompts ?? 0} prompts`);
        await refreshAll();
        closeModal();
      }
    });
    document.getElementById('set-unlock-session')?.addEventListener('click', async () => {
      closeModal();
      await unlockSessionDialog();
    });
    document.getElementById('set-lock-session')?.addEventListener('click', async () => {
      await api().session.lock();
      toast('Session locked');
      closeModal();
      if (state.selectedId) await selectPrompt(state.selectedId);
    });
    document.getElementById('set-forget-keychain')?.addEventListener('click', async () => {
      const ok = await confirmDialog({
        title: 'Forget keychain lock',
        message: 'Remove the saved lock password from the OS keychain?',
        detail: 'You will need to enter it again to unlock locked prompts.',
        confirmLabel: 'Forget password',
        danger: true,
      });
      if (!ok) return;
      await api().session.lock({ forgetKeychain: true });
      await api().settings.setRememberLock(false);
      toast('Keychain lock password cleared');
      closeModal();
    });
  });
}

function openBulkMove(): void {
  const opts = state.folders
    .map((f) => `<option value="${f.id}">${escapeHtml(f.name)}</option>`)
    .join('');
  showModal(
    'Move selected',
    `<p class="settings-lead">${state.selectedIds.size} prompt${state.selectedIds.size === 1 ? '' : 's'} selected.</p>
     <section class="settings-section">
       <h4 class="settings-section-title">Destination</h4>
       <label class="field">
         <span>Folder</span>
         <select id="bulk-folder" class="select">
           <option value="">— Unfiled —</option>${opts}
         </select>
       </label>
     </section>`,
    [
      { label: 'Cancel', className: 'btn btn-ghost', onClick: () => closeModal() },
      {
        label: 'Move',
        className: 'btn btn-primary',
        onClick: async () => {
          const v = (document.getElementById('bulk-folder') as HTMLSelectElement).value;
          await api().prompts.bulkMove([...state.selectedIds], v || null);
          closeModal();
          toast('Moved');
          await refreshAll();
        },
      },
    ],
  );
}

function unlockSessionDialog(): Promise<boolean> {
  return new Promise((resolve) => {
    showModal(
      'Unlock session',
      `<p class="settings-lead">Unlock locked prompts for this session. The password is never stored in MongrelDB.</p>
       <section class="settings-section">
         <h4 class="settings-section-title">Password</h4>
         <label class="field">
           <span>Session unlock password</span>
           <input id="unlock-pass" type="password" placeholder="Enter password" autocomplete="current-password" />
         </label>
         <div class="settings-toggle-row">
           <div class="settings-row-label">
             <strong>Remember in OS keychain</strong>
             <span>Auto-unlock on next launch</span>
           </div>
           <label class="toggle">
             <span>Remember</span>
             <input id="unlock-remember" type="checkbox" checked />
             <span class="toggle-ui"></span>
           </label>
         </div>
       </section>`,
      [
        {
          label: 'Cancel',
          className: 'btn btn-ghost',
          onClick: () => {
            closeModal();
            resolve(false);
          },
        },
        {
          label: 'Unlock',
          className: 'btn btn-primary',
          onClick: async () => {
            const pw = (document.getElementById('unlock-pass') as HTMLInputElement).value;
            const remember = (document.getElementById('unlock-remember') as HTMLInputElement)
              .checked;
            const res = await api().session.unlock(pw, { remember });
            if (!res.ok) {
              toast('Wrong password');
              resolve(false);
              return;
            }
            closeModal();
            toast(
              remember
                ? `Unlocked · saved to ${res.keychainBackend || 'keychain'}`
                : 'Session unlocked',
            );
            if (state.selectedId) await selectPrompt(state.selectedId);
            resolve(true);
          },
        },
      ],
    );
    queueMicrotask(() => (document.getElementById('unlock-pass') as HTMLInputElement)?.focus());
  });
}

function openFullscreen(which: 'body' | 'notes'): void {
  const src = which === 'body' ? ($('#ed-body') as HTMLTextAreaElement) : ($('#ed-notes') as HTMLTextAreaElement);
  const overlay = document.createElement('div');
  overlay.className = 'fs-overlay';
  overlay.innerHTML = `
    <header>
      <strong>${which === 'body' ? 'Prompt' : 'Notes'} — fullscreen</strong>
      <button type="button" class="btn btn-primary" id="fs-done">Done</button>
    </header>
    <textarea id="fs-area"></textarea>`;
  document.body.appendChild(overlay);
  const area = overlay.querySelector('#fs-area') as HTMLTextAreaElement;
  area.value = src.value;
  area.focus();
  overlay.querySelector('#fs-done')!.addEventListener('click', () => {
    src.value = area.value;
    if (which === 'body') $('#char-count').textContent = `${area.value.length} characters`;
    state.dirty = true;
    overlay.remove();
  });
}

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// Browser preview without Tauri: show shell chrome only
if (typeof window !== 'undefined') {
  if (window.promptsack) {
    void boot();
  } else {
    console.info('[PromptSack] Running without Tauri bridge (static preview)');
    $('#gate').hidden = true;
    $('#shell').hidden = false;
  }
}

export {};
