/**
 * Pure save-policy helpers shared by the renderer (and unit-tested).
 * Prevents wiping locked ciphertext by saving empty body/notes placeholders.
 */

import type { UpdatePromptInput } from './types.js';

export interface EditorSaveDraft {
  title: string;
  body: string;
  notes: string;
  tags: string[];
  folderId: string | null;
  favorite: boolean;
  locked: boolean;
  /** True when the editor is showing the locked placeholder (no plaintext loaded). */
  contentEncrypted: boolean;
  sessionUnlocked: boolean;
}

export type SavePolicyResult =
  | { ok: true; patch: UpdatePromptInput; needsReload: boolean }
  | { ok: false; reason: string; needsUnlock: boolean };

/**
 * Build an UpdatePromptInput from the editor draft.
 *
 * When content is still encrypted in the UI:
 * - refuse to save body/notes (would wipe ciphertext with empty strings)
 * - require session unlock first, then a reload so plaintext is in the form
 */
export function buildPromptSavePatch(draft: EditorSaveDraft): SavePolicyResult {
  if (draft.contentEncrypted) {
    if (!draft.sessionUnlocked) {
      return {
        ok: false,
        reason: 'Unlock the session before saving a locked prompt',
        needsUnlock: true,
      };
    }
    // Session is unlocked but UI still has placeholders — caller must reload first.
    return {
      ok: false,
      reason: 'Locked content was not loaded; unlock and reload before saving',
      needsUnlock: false,
    };
  }

  return {
    ok: true,
    needsReload: false,
    patch: {
      title: draft.title,
      body: draft.body,
      notes: draft.notes,
      tags: draft.tags,
      folderId: draft.folderId,
      favorite: draft.favorite,
      locked: draft.locked,
    },
  };
}
