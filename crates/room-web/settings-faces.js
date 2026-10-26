/**
 * Live face-swap settings (docs/26 §7.2 / F4).
 * Host wires these into game options / CVars.
 */

export const DEFAULT_FACE_SETTINGS = Object.freeze({
  /** Live static faces on enemies. */
  liveFaces: false,
  /** Optional live webcam revision of the active face. */
  liveWebcam: false,
  /** Hide faces received from peers. */
  hideSharedFaces: false,
  /** Offer our faces to the current room (requires server face_share_enabled). */
  shareMyFaces: false,
});

/**
 * @param {Partial<typeof DEFAULT_FACE_SETTINGS>} overrides
 */
export function mergeFaceSettings(overrides = {}) {
  return { ...DEFAULT_FACE_SETTINGS, ...overrides };
}
