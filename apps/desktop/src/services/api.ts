import { invoke } from "@tauri-apps/api/core";
import type { AccountInfo, AppSettings, AppState, DriveStatus, FilePreview, NodeInfo } from "../types";

export async function getAppState(): Promise<AppState> {
  return invoke("get_app_state");
}

export async function getSettings(): Promise<AppSettings> {
  return invoke("get_settings");
}

export async function updateSettings(settings: Partial<AppSettings>): Promise<AppSettings> {
  return invoke("update_settings", { patch: settings });
}

export async function listAccounts(): Promise<AccountInfo[]> {
  return invoke("list_accounts");
}

export async function connectGoogleAccount(): Promise<AccountInfo> {
  return invoke("connect_google_account");
}

export async function cancelGoogleOauth(): Promise<void> {
  return invoke("cancel_google_oauth");
}

export async function disconnectAccount(accountId: string): Promise<void> {
  return invoke("disconnect_account", { accountId });
}

export async function removeAccount(accountId: string): Promise<void> {
  return invoke("remove_account", { accountId });
}

export async function reconnectAccount(accountId: string): Promise<AccountInfo> {
  return invoke("reconnect_account", { accountId });
}

export async function syncMetadata(): Promise<void> {
  return invoke("sync_metadata");
}

export async function syncMetadataShallow(): Promise<void> {
  return invoke("sync_metadata_shallow");
}

export async function mountDrive(): Promise<DriveStatus> {
  return invoke("mount_drive");
}

export async function resolveUploadAccount(accountId: string): Promise<boolean> {
  return invoke("resolve_upload_account", { accountId });
}

export async function cancelUploadAccountPick(): Promise<boolean> {
  return invoke("cancel_upload_account_pick");
}

export async function uploadLocalPaths(
  paths: string[],
  parentPath?: string,
): Promise<number> {
  return invoke("upload_local_paths", {
    paths,
    parentPath: parentPath ?? null,
  });
}

export async function unmountDrive(): Promise<DriveStatus> {
  return invoke("unmount_drive");
}

export async function listDirectory(path?: string): Promise<NodeInfo[]> {
  return invoke("list_directory", { path: path ?? null });
}

export async function ensureDirectory(path?: string): Promise<number> {
  return invoke("ensure_directory", { path: path ?? null });
}

export async function searchNodes(query: string): Promise<NodeInfo[]> {
  return invoke("search_nodes", { query });
}

export async function refreshAccountQuotas(): Promise<AccountInfo[]> {
  return invoke("refresh_account_quotas");
}

export async function readFilePreview(
  virtualPath: string,
  maxBytes?: number,
): Promise<FilePreview> {
  return invoke("read_file_preview", {
    virtualPath,
    maxBytes: maxBytes ?? null,
  });
}

export async function createStorbyteLink(virtualPath: string, destDir: string): Promise<string> {
  return invoke("create_storbyte_link", { virtualPath, destDir });
}

export async function createStorbyteLinkDesktop(virtualPath: string): Promise<string> {
  return invoke("create_storbyte_link_desktop", { virtualPath });
}

export async function openStorbyteLink(path: string): Promise<void> {
  return invoke("open_storbyte_link", { path });
}

export async function availableDriveLetters(): Promise<string[]> {
  return invoke("available_drive_letters");
}

export async function winfspInstalled(): Promise<boolean> {
  return invoke("winfsp_installed");
}

export async function installWinfsp(): Promise<boolean> {
  return invoke("install_winfsp");
}

export async function openWinfspInstaller(): Promise<void> {
  return invoke("open_winfsp_installer");
}
