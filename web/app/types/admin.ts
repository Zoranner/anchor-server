export type DeviceStatus = 'pending' | 'active' | 'rejected' | 'disabled'
export type PolicyStatus = 'draft' | 'signed'
export type ActionPolicy = 'allow' | 'deny'
export type ScanRuleKind = 'keyword' | 'regex'

export interface AdminUser {
  id: string
  username: string
  role: string
  created_at: string
}

export interface SigningKey {
  key_id: string
  public_key_hex: string
  created_at: string
  activated_at: string
}

export interface AdminMeResponse {
  admin: AdminUser
  signing_key: SigningKey | null
}

export interface AdminSessionResponse extends AdminMeResponse {
  expires_at: string
}

export interface ApiPage<T> {
  items: T[]
  next_cursor: string | null
}

export interface InventorySummary {
  computer_model: string
  os_name: string
  os_version: string
  cpu_model: string
  memory_bytes: number
  disk_count: number
  network_adapter_count: number
}

export interface EffectivePolicy {
  policy_id: string
  version: number
  expires_at: string
}

export interface DeviceSummary {
  id: string
  installation_id: string
  hostname: string
  status: DeviceStatus
  client_version: string | null
  last_seen_at: string | null
  created_at: string
  approved_at: string | null
  inventory_summary: InventorySummary | null
}

export interface DeviceDetail extends Omit<DeviceSummary, 'inventory_summary'> {
  disabled_at: string | null
  inventory: HardwareInventory | null
  inventory_captured_at: string | null
  inventory_received_at: string | null
  effective_policy: EffectivePolicy | null
  last_policy_version: number | null
  last_policy_fetched_at: string | null
}

export interface DeviceActionResponse {
  id: string
  status: DeviceStatus
}

export interface ComputerInventory {
  manufacturer: string
  model: string
  serial_number: string
  bios_vendor: string
  bios_version: string
  bios_release_date: string
}

export interface OperatingSystemInventory {
  name: string
  version: string
  build: string
  architecture: string
  locale: string
  hostname: string
}

export interface CpuInventory {
  manufacturer: string
  model: string
  cores: number
  threads: number
}

export interface MemoryModuleInventory {
  manufacturer: string
  part_number: string
  serial_number: string
  capacity_bytes: number
  speed_mhz: number
  slot: string
}

export interface DiskInventory {
  model: string
  serial_number: string
  media_type: string
  size_bytes: number
}

export interface VolumeInventory {
  name: string
  label: string
  filesystem: string
  size_bytes: number
  free_bytes: number
  serial_number: string
}

export interface NetworkAdapterInventory {
  instance_id: string
  name: string
  mac_address: string
  link_up: boolean
  driver_version: string
}

export interface UsbDeviceInventory {
  instance_id: string
  name: string
  manufacturer: string
  vendor_id: string
  product_id: string
  serial_number: string
  device_class: string
}

export interface OpticalDriveInventory {
  instance_id: string
  name: string
  media_present: boolean
}

export interface GpuInventory {
  name: string
  manufacturer: string
  memory_bytes: number
  driver_version: string
}

export interface HardwareInventory {
  schema_version: number
  captured_at: string
  computer: ComputerInventory
  operating_system: OperatingSystemInventory
  cpu: CpuInventory
  memory_modules: MemoryModuleInventory[]
  disks: DiskInventory[]
  volumes: VolumeInventory[]
  network_adapters: NetworkAdapterInventory[]
  usb_devices: UsbDeviceInventory[]
  optical_drives: OpticalDriveInventory[]
  gpus: GpuInventory[]
}

export interface UsbAllowRule {
  instance_id: string | null
  vendor_id: string | null
  product_id: string | null
  serial: string | null
}

export interface NetworkAllowRule {
  adapter_id: string
}

export interface BurningAllowRule {
  drive_id: string
}

export interface UsbRules {
  read: ActionPolicy
  write: ActionPolicy
  allowed: UsbAllowRule[]
}

export interface NetworkRules {
  policy: ActionPolicy
  allowed: NetworkAllowRule[]
}

export interface BurningRules {
  policy: ActionPolicy
  allowed: BurningAllowRule[]
}

export interface ScanRule {
  rule_id: string
  kind: ScanRuleKind
  pattern: string
}

export interface ScanRules {
  controlled_dirs: string[]
  file_types: string[]
  rules: ScanRule[]
}

export interface PolicyPayload {
  version: number
  expires_at: string
  usb: UsbRules
  network: NetworkRules
  burning: BurningRules
  scan: ScanRules
}

export interface PolicyDraftRequest {
  expires_at: string
  usb: UsbRules
  network: NetworkRules
  burning: BurningRules
  scan: ScanRules
}

export interface PolicySummary {
  id: string
  version: number | null
  status: PolicyStatus
  expires_at: string
  created_by_admin_id: string
  created_at: string
  signed_at: string | null
  server_key_id: string | null
}

export interface PolicyDetail extends PolicySummary {
  policy_json: PolicyPayload
  signature_hex: string | null
}

export interface AssignPolicyRequest {
  device_id?: string
  is_default?: boolean
}

export interface ApiErrorBody {
  error?: {
    code?: string
    message?: string
    details?: unknown
  }
}
