/**
 * Debug configuration and mock data for development.
 * This module is dynamically imported only in development mode
 * to avoid shipping debug fixtures in production builds.
 */
import type { AudioSession } from "$lib/types";
import { SYSTEM_VOLUME_PROCESS_NAME } from "$lib/stores/audioStore";

export const DEBUG = {
  ENABLED: false, // Global debugging toggle
  FORCE_BOOT_SCREEN: false, // Force application to hang on boot screen
  FORCE_BOOT_ERROR: false, // Force application to display boot error with restart button
  FORCE_AUDIO_NOT_INITIALISED: false, // Force application to behave as if audio subsystem failed to initialise
  FORCE_EDIT_MODE: false, // Force application to start in edit mode
  FORCE_NO_SESSIONS: false, // Force application to start with no audio sessions
  FORCE_ERROR_BANNER: false, // Force application to display error banner
  ERROR_BANNER_TEXT: "Critical error", // Text to display in error banner
  FORCE_WARNING_BANNER: false, // Force application to display warning banner
  WARNING_BANNER_TEXT: "Update available", // Text to display in warning banner
  FORCE_MOCK_SESSIONS: false, // Force application to use mock audio sessions for testing UI without actual audio subsystem (overrides FORCE_NO_SESSIONS)
  MOCK_SESSIONS: [
    { session_id: "mock_1", display_name: "Discord", process_id: 1234, process_name: "discord.exe", volume: 0.75, is_muted: false },
    { session_id: "mock_2", display_name: "Spotify", process_id: 5678, process_name: "spotify.exe", volume: 0.50, is_muted: false },
    { session_id: "mock_3", display_name: "Microsoft Flight Simulator", process_id: 9012, process_name: "flightsim.exe", volume: 1.0, is_muted: true },
    { session_id: "mock_4", display_name: "Google Chrome", process_id: 3456, process_name: "chrome.exe", volume: 0.25, is_muted: false },
    { session_id: "mock_5", display_name: "System Sounds", process_id: 0, process_name: SYSTEM_VOLUME_PROCESS_NAME, volume: 0.80, is_muted: false },
    { session_id: "mock_6", display_name: "Game Launcher", process_id: 7890, process_name: "launcher1.exe", volume: 0.60, is_muted: false },
    { session_id: "mock_7", display_name: "Video Player", process_id: 2345, process_name: "videoplayer1.exe", volume: 0.90, is_muted: false },
    { session_id: "mock_8", display_name: "Communication App", process_id: 6789, process_name: "commapp1.exe", volume: 0.40, is_muted: true },
    { session_id: "mock_9", display_name: "Music Player", process_id: 1357, process_name: "musicplayer1.exe", volume: 0.55, is_muted: false },
    { session_id: "mock_10", display_name: "Video Conferencing", process_id: 2468, process_name: "videoconf1.exe", volume: 0.65, is_muted: false },
    { session_id: "mock_11", display_name: "Audio Editor", process_id: 1122, process_name: "audioeditor1.exe", volume: 0.85, is_muted: false },
    { session_id: "mock_12", display_name: "Streaming Software", process_id: 3344, process_name: "streamer1.exe", volume: 0.70, is_muted: false },
    { session_id: "mock_13", display_name: "Virtual Machine", process_id: 5566, process_name: "vm1.exe", volume: 0.30, is_muted: true },
    { session_id: "mock_14", display_name: "Browser Music Tab", process_id: 7788, process_name: "chrome1.exe", volume: 0.45, is_muted: false },
    { session_id: "mock_15", display_name: "Game Soundtrack", process_id: 9900, process_name: "gamesoundtrack1.exe", volume: 0.95, is_muted: false },
    { session_id: "mock_16", display_name: "Podcast App", process_id: 2233, process_name: "podcastapp1.exe", volume: 0.35, is_muted: false },
    { session_id: "mock_17", display_name: "Voice Chat", process_id: 4455, process_name: "voicechat1.exe", volume: 0.50, is_muted: true },
    { session_id: "mock_18", display_name: "System Notifications", process_id: 0, process_name: "sysnotif1.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_19", display_name: "Media Player", process_id: 6677, process_name: "mediaplayer1.exe", volume: 0.60, is_muted: false },
    { session_id: "mock_20", display_name: "Audio Books", process_id: 8899, process_name: "audiobooks1.exe", volume: 0.40, is_muted: true },
    { session_id: "mock_21", display_name: "System Volume 2", process_id: 0, process_name: "sysvolume2.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_22", display_name: "Background Music", process_id: 1010, process_name: "bgmusic1.exe", volume: 0.20, is_muted: false },
    { session_id: "mock_23", display_name: "Game Chat", process_id: 2020, process_name: "gamechat1.exe", volume: 0.75, is_muted: true },
    { session_id: "mock_24", display_name: "Video Call", process_id: 3030, process_name: "videocall1.exe", volume: 0.65, is_muted: false },
    { session_id: "mock_25", display_name: "Music Streaming", process_id: 4040, process_name: "musicstreaming1.exe", volume: 0.55, is_muted: false },
    { session_id: "mock_26", display_name: "Voice Assistant", process_id: 5050, process_name: "assistant1.exe", volume: 0.45, is_muted: true },
    { session_id: "mock_27", display_name: "System Sounds 2", process_id: 0, process_name: "sysnotif2.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_28", display_name: "Video Streaming", process_id: 6060, process_name: "videostream1.exe", volume: 0.85, is_muted: false },
    { session_id: "mock_29", display_name: "Audio Mixer", process_id: 7070, process_name: "audiomixer1.exe", volume: 0.70, is_muted: false },
    { session_id: "mock_30", display_name: "Game Audio", process_id: 8080, process_name: "gameaudio1.exe", volume: 0.90, is_muted: true },
    { session_id: "mock_31", display_name: "System Volume 3", process_id: 0, process_name: "sysvolume3.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_32", display_name: "Music Player 2", process_id: 9090, process_name: "musicplayer2.exe", volume: 0.60, is_muted: false },
    { session_id: "mock_33", display_name: "Video Editor", process_id: 1111, process_name: "videoeditor1.exe", volume: 0.75, is_muted: false },
    { session_id: "mock_34", display_name: "Communication App 2", process_id: 2222, process_name: "commapp2.exe", volume: 0.50, is_muted: true },
    { session_id: "mock_35", display_name: "System Sounds 3", process_id: 0, process_name: "sysnotif3.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_36", display_name: "Game Launcher 2", process_id: 3333, process_name: "launcher2.exe", volume: 0.65, is_muted: false },
    { session_id: "mock_37", display_name: "Video Player 2", process_id: 4444, process_name: "videoplayer2.exe", volume: 0.85, is_muted: false },
    { session_id: "mock_38", display_name: "Voice Chat 2", process_id: 5555, process_name: "voicechat2.exe", volume: 0.40, is_muted: true },
    { session_id: "mock_39", display_name: "Music Player 3", process_id: 6666, process_name: "musicplayer3.exe", volume: 0.55, is_muted: false },
    { session_id: "mock_40", display_name: "Video Conferencing 2", process_id: 7777, process_name: "videoconf2.exe", volume: 0.70, is_muted: false },
    { session_id: "mock_41", display_name: "Audio Editor 2", process_id: 8888, process_name: "audioeditor2.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_42", display_name: "Streaming Software 2", process_id: 9999, process_name: "streamer2.exe", volume: 0.75, is_muted: false },
    { session_id: "mock_43", display_name: "Virtual Machine 2", process_id: 1212, process_name: "vm2.exe", volume: 0.30, is_muted: true },
    { session_id: "mock_44", display_name: "Browser Music Tab 2", process_id: 1313, process_name: "chrome2.exe", volume: 0.45, is_muted: false },
    { session_id: "mock_45", display_name: "Game Soundtrack 2", process_id: 1414, process_name: "gamesoundtrack2.exe", volume: 0.95, is_muted: false },
    { session_id: "mock_46", display_name: "Podcast App 2", process_id: 1515, process_name: "podcastapp2.exe", volume: 0.35, is_muted: false },
    { session_id: "mock_47", display_name: "Voice Chat 3", process_id: 1616, process_name: "voicechat3.exe", volume: 0.50, is_muted: true },
    { session_id: "mock_48", display_name: "System Notifications 2", process_id: 0, process_name: "sysnotif4.exe", volume: 0.80, is_muted: false },
    { session_id: "mock_49", display_name: "Media Player 2", process_id: 1717, process_name: "mediaplayer2.exe", volume: 0.60, is_muted: false },
    { session_id: "mock_50", display_name: "Audio Books 2", process_id: 1818, process_name: "audiobooks2.exe", volume: 0.40, is_muted: true }
  ] as AudioSession[],
} as const;
