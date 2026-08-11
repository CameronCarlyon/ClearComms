/**
 * Aircraft Profile Library
 *
 * A curated, hand-maintained registry mapping generic radio channel categories
 * (COM1, COM2, COM3, HF1, HF2, CAB, PA, INT) to the aircraft-specific LVars
 * that drive them, for each supported aircraft.
 *
 * Source data is verified against the MobiFlight HubHop preset database
 * (https://hubhop.mobiflight.com). LVars that could not be verified are
 * deliberately omitted (e.g. a missing mute definition) rather than guessed —
 * subscribing to a non-existent LVar returns a constant 0, which would be
 * misinterpreted as "muted" and would falsely mute the linked application.
 *
 * The Rust backend holds no aircraft knowledge: the frontend resolves the
 * active profile from the TITLE SimVar and sends the backend a flat list of
 * LVar names to subscribe to.
 */
import type { SimChannelCategory, SimSeat } from '$lib/types';

/** A readable/writable volume knob LVar with its native value range */
export interface VolumeEndpoint {
  lvar: string;
  min: number;
  max: number;
}

/**
 * A receive/mute switch LVar (the push/pull knob on the audio panel).
 * Values are explicit so either polarity can be expressed.
 */
export interface MuteEndpoint {
  lvar: string;
  /** Raw LVar value when the channel is muted (receive switch pushed in/off) */
  mutedValue: number;
  /** Raw LVar value when the channel is unmuted (receive switch pulled out/on) */
  unmutedValue: number;
}

/** The volume and (optional) mute endpoints for one channel category */
export interface SimChannelDef {
  volume: VolumeEndpoint;
  mute?: MuteEndpoint;
}

/** Channel definitions for one seat — categories may be absent if unsupported */
export type SeatChannels = Partial<Record<SimChannelCategory, SimChannelDef>>;

export interface AircraftProfile {
  id: string;
  name: string;
  /** Case-insensitive regex patterns tested against the simulator TITLE SimVar */
  titlePatterns: string[];
  seats: Record<SimSeat, SeatChannels>;
}

// ─────────────────────────────────────────────────────────────────────────────
// Seed Profiles
// ─────────────────────────────────────────────────────────────────────────────

/** Fenix A320 family (ACP1 = captain, ACP2 = first officer). Volume range 0–1. */
const FENIX_A320: AircraftProfile = {
  id: 'fenix-a320',
  name: 'Fenix A320',
  titlePatterns: ['fenix'],
  seats: {
    captain: {
      COM1: { volume: { lvar: 'A_ASP_VHF_1_VOLUME', min: 0, max: 1 }, mute: { lvar: 'S_ASP_VHF_1_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      COM2: { volume: { lvar: 'A_ASP_VHF_2_VOLUME', min: 0, max: 1 }, mute: { lvar: 'S_ASP_VHF_2_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      COM3: { volume: { lvar: 'A_ASP_VHF_3_VOLUME', min: 0, max: 1 }, mute: { lvar: 'S_ASP_VHF_3_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      HF1:  { volume: { lvar: 'A_ASP_HF_1_VOLUME', min: 0, max: 1 },  mute: { lvar: 'S_ASP_HF_1_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      HF2:  { volume: { lvar: 'A_ASP_HF_2_VOLUME', min: 0, max: 1 },  mute: { lvar: 'S_ASP_HF_2_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      CAB:  { volume: { lvar: 'A_ASP_CAB_VOLUME', min: 0, max: 1 },   mute: { lvar: 'S_ASP_CAB_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      PA:   { volume: { lvar: 'A_ASP_PA_VOLUME', min: 0, max: 1 },    mute: { lvar: 'S_ASP_PA_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
      INT:  { volume: { lvar: 'A_ASP_INT_VOLUME', min: 0, max: 1 },   mute: { lvar: 'S_ASP_INT_REC_LATCH', mutedValue: 0, unmutedValue: 1 } },
    },
    // First officer: A_ASP2_* volume vars are verified in HubHop, but no ACP2
    // receive-switch vars are published, so mute is unavailable on this seat.
    firstOfficer: {
      COM1: { volume: { lvar: 'A_ASP2_VHF_1_VOLUME', min: 0, max: 1 } },
      COM2: { volume: { lvar: 'A_ASP2_VHF_2_VOLUME', min: 0, max: 1 } },
      COM3: { volume: { lvar: 'A_ASP2_VHF_3_VOLUME', min: 0, max: 1 } },
      HF1:  { volume: { lvar: 'A_ASP2_HF_1_VOLUME', min: 0, max: 1 } },
      HF2:  { volume: { lvar: 'A_ASP2_HF_2_VOLUME', min: 0, max: 1 } },
      CAB:  { volume: { lvar: 'A_ASP2_CAB_VOLUME', min: 0, max: 1 } },
      PA:   { volume: { lvar: 'A_ASP2_PA_VOLUME', min: 0, max: 1 } },
      INT:  { volume: { lvar: 'A_ASP2_INT_VOLUME', min: 0, max: 1 } },
    },
  },
};

/** FlyByWire A380X (RMP 1 = captain, RMP 2 = first officer). Volume range 0–100. */
const FBW_A380X: AircraftProfile = {
  id: 'fbw-a380x',
  name: 'FlyByWire A380X',
  titlePatterns: ['a380x'],
  seats: {
    captain: {
      COM1: { volume: { lvar: 'A380X_RMP_1_VHF_VOL_1', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_1_VHF_VOL_RX_SWITCH_1', mutedValue: 0, unmutedValue: 1 } },
      COM2: { volume: { lvar: 'A380X_RMP_1_VHF_VOL_2', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_1_VHF_VOL_RX_SWITCH_2', mutedValue: 0, unmutedValue: 1 } },
      COM3: { volume: { lvar: 'A380X_RMP_1_VHF_VOL_3', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_1_VHF_VOL_RX_SWITCH_3', mutedValue: 0, unmutedValue: 1 } },
      // HF, CAB, PA and INT have volume knobs but no published receive-switch
      // LVars on the A380X, so they are volume-only.
      HF1:  { volume: { lvar: 'A380X_RMP_1_HF_VOL_1', min: 0, max: 100 } },
      HF2:  { volume: { lvar: 'A380X_RMP_1_HF_VOL_2', min: 0, max: 100 } },
      CAB:  { volume: { lvar: 'A380X_RMP_1_CAB_VOL', min: 0, max: 100 } },
      PA:   { volume: { lvar: 'A380X_RMP_1_PA_VOL', min: 0, max: 100 } },
      INT:  { volume: { lvar: 'A380X_RMP_1_INT_VOL', min: 0, max: 100 } },
    },
    firstOfficer: {
      COM1: { volume: { lvar: 'A380X_RMP_2_VHF_VOL_1', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_2_VHF_VOL_RX_SWITCH_1', mutedValue: 0, unmutedValue: 1 } },
      COM2: { volume: { lvar: 'A380X_RMP_2_VHF_VOL_2', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_2_VHF_VOL_RX_SWITCH_2', mutedValue: 0, unmutedValue: 1 } },
      COM3: { volume: { lvar: 'A380X_RMP_2_VHF_VOL_3', min: 0, max: 100 }, mute: { lvar: 'A380X_RMP_2_VHF_VOL_RX_SWITCH_3', mutedValue: 0, unmutedValue: 1 } },
      HF1:  { volume: { lvar: 'A380X_RMP_2_HF_VOL_1', min: 0, max: 100 } },
      HF2:  { volume: { lvar: 'A380X_RMP_2_HF_VOL_2', min: 0, max: 100 } },
      CAB:  { volume: { lvar: 'A380X_RMP_2_CAB_VOL', min: 0, max: 100 } },
      PA:   { volume: { lvar: 'A380X_RMP_2_PA_VOL', min: 0, max: 100 } },
      INT:  { volume: { lvar: 'A380X_RMP_2_INT_VOL', min: 0, max: 100 } },
    },
  },
};

/**
 * iniBuilds A350 (RMP 1 = captain, RMP 2 = first officer). Volume range 0–100.
 * The audio panel uses channel-indexed LVars: 1–3 = VHF 1–3, 4–5 = HF 1–2,
 * 8 = INT, 9 = PA, 11 = CAB.
 */
const INIBUILDS_A350: AircraftProfile = {
  id: 'inibuilds-a350',
  name: 'iniBuilds A350',
  titlePatterns: ['a350'],
  seats: {
    captain: {
      COM1: { volume: { lvar: 'INI_RMP1_1_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_1_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      COM2: { volume: { lvar: 'INI_RMP1_2_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_2_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      COM3: { volume: { lvar: 'INI_RMP1_3_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_3_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      HF1:  { volume: { lvar: 'INI_RMP1_4_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_4_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      HF2:  { volume: { lvar: 'INI_RMP1_5_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_5_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      INT:  { volume: { lvar: 'INI_RMP1_8_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_8_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      PA:   { volume: { lvar: 'INI_RMP1_9_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_9_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      CAB:  { volume: { lvar: 'INI_RMP1_11_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP1_11_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
    },
    firstOfficer: {
      COM1: { volume: { lvar: 'INI_RMP2_1_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_1_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      COM2: { volume: { lvar: 'INI_RMP2_2_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_2_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      COM3: { volume: { lvar: 'INI_RMP2_3_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_3_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      HF1:  { volume: { lvar: 'INI_RMP2_4_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_4_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      HF2:  { volume: { lvar: 'INI_RMP2_5_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_5_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      INT:  { volume: { lvar: 'INI_RMP2_8_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_8_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      PA:   { volume: { lvar: 'INI_RMP2_9_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_9_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
      CAB:  { volume: { lvar: 'INI_RMP2_11_VOLUME', min: 0, max: 100 }, mute: { lvar: 'INI_RMP2_11_RECEIVE_SEL', mutedValue: 0, unmutedValue: 1 } },
    },
  },
};

// ─────────────────────────────────────────────────────────────────────────────
// Registry & Lookup Helpers
// ─────────────────────────────────────────────────────────────────────────────

export const AIRCRAFT_PROFILES: AircraftProfile[] = [
  FENIX_A320,
  FBW_A380X,
  INIBUILDS_A350,
];

/** All channel categories in display order (for pickers) */
export const SIM_CHANNEL_CATEGORIES: SimChannelCategory[] = [
  'COM1', 'COM2', 'COM3', 'HF1', 'HF2', 'CAB', 'PA', 'INT',
];

/**
 * Match an aircraft TITLE SimVar string to a profile. Returns null when the
 * aircraft is unsupported (sim channel features are then unavailable).
 */
export function matchAircraftProfile(title: string | null): AircraftProfile | null {
  if (!title) return null;
  for (const profile of AIRCRAFT_PROFILES) {
    for (const pattern of profile.titlePatterns) {
      if (new RegExp(pattern, 'i').test(title)) return profile;
    }
  }
  return null;
}

export function getProfileById(id: string): AircraftProfile | null {
  return AIRCRAFT_PROFILES.find(p => p.id === id) ?? null;
}

/** Resolve the channel definition for a profile/seat/category combination */
export function getChannelDef(
  profile: AircraftProfile,
  seat: SimSeat,
  category: SimChannelCategory,
): SimChannelDef | null {
  return profile.seats[seat][category] ?? null;
}

/** Categories the profile actually supports for the given seat, in display order */
export function getSupportedCategories(profile: AircraftProfile, seat: SimSeat): SimChannelCategory[] {
  return SIM_CHANNEL_CATEGORIES.filter(c => profile.seats[seat][c] !== undefined);
}

/** Map a raw LVar value to a normalised 0–1 unit volume */
export function normaliseVolume(raw: number, endpoint: VolumeEndpoint): number {
  const span = endpoint.max - endpoint.min;
  if (span <= 0) return 0;
  return Math.min(1, Math.max(0, (raw - endpoint.min) / span));
}

/**
 * Map a normalised 0–1 unit volume back to the LVar's native range.
 * Wide ranges (e.g. 0–100) are integer knobs, so the result is rounded;
 * narrow ranges (0–1) are analogue and keep full precision.
 */
export function denormaliseVolume(unit: number, endpoint: VolumeEndpoint): number {
  const clamped = Math.min(1, Math.max(0, unit));
  const raw = endpoint.min + clamped * (endpoint.max - endpoint.min);
  return endpoint.max - endpoint.min > 10 ? Math.round(raw) : raw;
}
