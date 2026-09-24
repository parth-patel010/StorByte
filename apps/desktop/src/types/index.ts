export type UploadPlacement = "most_free" | "least_free" | "ask";

export type AppSettings = {
  drive_letter: string;
  zero_bit_export: boolean;
  cache_bytes: number;
  default_account_id: string | null;
  onboarded: boolean;
  intro_seen: boolean;
  upload_placement: UploadPlacement;
  launch_at_startup: boolean;
  auto_mount_on_launch: boolean;
};

export type AccountInfo = {
  id: string;
  email: string;
  provider: string;
  status: string;
  priority: number;
  quota_bytes: number | null;
  usage_bytes: number | null;
  last_indexed_at: string | null;
};

export type AccountSyncStatus = {
  label: string;
  status: string;
  items_indexed: number;
};

export type SyncProgress = {
  active: boolean;
  percent: number;
  items_indexed: number;
  folders_done: number;
  folders_pending: number;
  account_index: number;
  account_total: number;
  account_label: string;
  eta_secs: number | null;
  message: string;
  accounts: AccountSyncStatus[];
};

export type DriveStatus = {
  mounted: boolean;
  drive_letter: string;
  indexing: boolean;
  last_error: string | null;
  sync: SyncProgress | null;
  waiting_for_network: boolean;
};

export type NodeInfo = {
  id: number;
  parent_id: number | null;
  name: string;
  virtual_path: string;
  is_directory: boolean;
  size: number;
  mime_type: string | null;
  remote_id: string;
  account_id: string;
  etag: string | null;
  modified_at: string | null;
  last_indexed: string | null;
};

export type FilePreview = {
  name: string;
  mime_type: string | null;
  size: number;
  account_id: string;
  virtual_path: string;
  data_base64: string;
  truncated: boolean;
  on_disk_bytes: number;
};

export type AppState = {
  settings: AppSettings;
  accounts: AccountInfo[];
  drive: DriveStatus;
};
