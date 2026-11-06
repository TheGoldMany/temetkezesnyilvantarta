// Mirrors crates/lunda-core/src/model.rs (camelCase JSON).

export type PlotStatus = "free" | "reserved" | "occupied" | "expired" | "notified" | "resellable" | "liquidated" | "unusable";
export const PLOT_STATUSES: PlotStatus[] = ["free", "reserved", "occupied", "expired", "notified", "resellable", "liquidated", "unusable"];

export type EventType =
  | "coffin_burial"
  | "urn_placement"
  | "reburial"
  | "urn_release"
  | "urn_removal"
  | "ash_scattering"
  | "relocation"
  | "common_grave";
export const EVENT_TYPES: EventType[] = ["coffin_burial", "urn_placement", "reburial", "urn_release", "urn_removal", "ash_scattering", "relocation", "common_grave"];

export type Role = "admin" | "clerk" | "finance" | "reader" | "cashier" | "worker" | "undertaker";
export type Permission =
  | "manage_users"
  | "manage_settings"
  | "manage_structure"
  | "record_events"
  | "record_leases"
  | "record_payments"
  | "cash_desk"
  | "operations"
  | "complete_work"
  | "calendar"
  | "correct_entries"
  | "void_entries"
  | "view_personal"
  | "print"
  | "export"
  | "import"
  | "backup"
  | "edit_map"
  | "override_checks";
export const PERMISSIONS: Permission[] = [
  "manage_users",
  "manage_settings",
  "manage_structure",
  "record_events",
  "record_leases",
  "record_payments",
  "cash_desk",
  "operations",
  "complete_work",
  "calendar",
  "correct_entries",
  "void_entries",
  "view_personal",
  "print",
  "export",
  "import",
  "backup",
  "edit_map",
  "override_checks",
];

export interface Session {
  userId: string | null;
  userName: string;
  displayName: string;
  role: Role;
  permissions: Permission[];
}

export interface CoreError {
  code: string;
  message: string;
  params?: Record<string, string>;
}

export interface AppStatus {
  version: string;
  dbReady: boolean;
  dbError: CoreError | null;
  hasUsers: boolean;
  loginRequired: boolean;
  session: Session | null;
  dataDir: string;
  settings: Record<string, string>;
  backupWarning: boolean;
  /** signed-in web account: verification, two-factor, trial */
  account?: AccountInfo;
}

export interface AccountInfo {
  email: string;
  organization: string;
  emailVerified: boolean;
  totpEnabled: boolean;
  totpRequired: boolean;
  tenantStatus: "trial" | "active" | "read_only" | "closed";
  trialEndsAt: string;
  trialDaysLeft: number;
  superadmin?: boolean;
}

/** First sign-in step answer when an authenticator code is needed. */
export interface MfaChallenge {
  mfa: true;
  challenge: string;
}

export interface Cemetery {
  id: string;
  name: string;
  address: string;
  parcelNumber: string;
  kind: "public" | "denominational" | string;
  owner: string;
  operator: string;
  openingHours: string;
  folioScope: "continuous" | "yearly" | string;
  notes: string;
  voidedAt?: string | null;
}

export interface PlotType {
  id: string;
  code: string;
  name: string;
  category: "grave" | "crypt" | "urn" | "scatter" | "common" | string;
  defaultLeaseYears: number;
  restPeriodYears: number;
  coffinCapacity: number;
  urnCapacity: number;
  widthCm: number | null;
  lengthCm: number | null;
  builtin: boolean;
  sortOrder: number;
}

export interface Section {
  id: string;
  cemeteryId: string;
  code: string;
  name: string;
  mapX: number;
  mapY: number;
  mapW: number;
  mapH: number;
  mapRot: number;
  rowCount: number;
  plotCount: number;
}

export interface PlotRow {
  id: string;
  sectionId: string;
  number: string;
  direction: string;
}

export interface Plot {
  id: string;
  cemeteryId: string;
  sectionId: string;
  rowId: string;
  number: string;
  identifier: string;
  plotTypeId: string;
  coffinCapacity: number;
  urnCapacity: number;
  widthCm: number | null;
  lengthCm: number | null;
  restPeriodYears: number | null;
  statusOverride: string | null;
  statusOverrideReason: string | null;
  unusable: boolean;
  protected: boolean;
  protectionKind: "national" | "local" | null;
  protectionReason: string | null;
  furtherBurialAllowed: boolean;
  resaleReadyAt: string | null;
  notes: string;
  mapX: number;
  mapY: number;
  mapW: number;
  mapH: number;
  mapRot: number;
  mergedIntoId: string | null;
  gpsLat: number | null;
  gpsLon: number | null;
  cryptNumber: string | null;
  cryptBuiltYear: number | null;
  cryptPermit: string | null;
  cryptHolderInstructions: string | null;
  voidedAt: string | null;
}

export interface PlotSummary {
  id: string;
  identifier: string;
  cemeteryId: string;
  sectionId: string;
  rowId: string;
  typeCode: string;
  typeName: string;
  status: PlotStatus;
  protected: boolean;
  occupants: number;
  leaseExpiry: string | null;
  holderName: string | null;
  occupantNames: string | null;
  x: number;
  y: number;
  w: number;
  h: number;
  rot: number;
}

export interface Restriction {
  id: string;
  plotId: string;
  kind: "ban" | "condition";
  text: string;
  validFrom: string | null;
  validTo: string | null;
  voidedAt: string | null;
}

export interface Monument {
  id: string;
  plotId: string;
  entryDate: string;
  description: string;
  contractor: string;
  material: string;
  size: string;
  permitDate: string | null;
  protected: boolean;
  voidedAt: string | null;
}

export interface CryptSlot {
  id: string;
  plotId: string;
  slotNo: number;
  label: string;
  occupantName: string | null;
  occupantEventId: string | null;
  occupantFolio: number | null;
  occupantDate: string | null;
}

export interface Deceased {
  id: string;
  namePrefix: string;
  familyName: string;
  givenName: string;
  birthName: string;
  sex: "" | "male" | "female" | "unknown";
  birthPlace: string;
  birthDate: string | null;
  motherName: string;
  lastResidence: string;
  deathPlace: string;
  deathDate: string | null;
  deathCertNo: string;
  unknownIdentity: boolean;
  authorityName: string;
  caseNumber: string;
  notes: string;
  hidePublic?: boolean;
  voidedAt?: string | null;
}

export interface Person {
  id: string;
  namePrefix: string;
  familyName: string;
  givenName: string;
  address: string;
  mailAddress: string;
  phone: string;
  email: string;
  notes: string;
  masked: boolean;
  voidedAt?: string | null;
}

export interface InterEvent {
  id: string;
  cemeteryId: string;
  scopeKey: string;
  folio: number;
  historical: boolean;
  eventType: EventType;
  container: string;
  eventDate: string;
  deceasedId: string;
  plotId: string | null;
  cryptSlotId: string | null;
  arrangerId: string | null;
  receiverName: string;
  receiverAddress: string;
  destination: string;
  sourceEventId: string | null;
  overrideReason: string | null;
  notes: string;
  voidedAt: string | null;
  voidReason: string | null;
}

export interface EventRow {
  id: string;
  cemeteryId: string;
  cemeteryName: string;
  folio: number;
  scopeKey: string;
  historical: boolean;
  eventType: EventType;
  container: string;
  eventDate: string;
  deceasedId: string;
  deceasedName: string;
  birthName: string;
  birthDate: string | null;
  birthPlace: string;
  motherName: string;
  lastResidence: string;
  deathDate: string | null;
  unknownIdentity: boolean;
  authorityName: string;
  caseNumber: string;
  plotId: string | null;
  plotIdentifier: string | null;
  cryptSlotNo: number | null;
  arrangerName: string | null;
  arrangerAddress: string | null;
  receiverName: string;
  destination: string;
  notes: string;
  voidedAt: string | null;
  voidReason: string | null;
}

export interface Holder {
  id: string;
  plotId: string;
  personId: string;
  personName: string;
  personAddress: string;
  role: "primary" | "contact";
  relation: string;
  startDate: string;
  endDate: string | null;
  legalBasis: string;
  documentRef: string;
  plotIdentifier: string | null;
  voidedAt: string | null;
}

export interface Lease {
  id: string;
  plotId: string;
  personId: string | null;
  personName: string | null;
  kind: "purchase" | "prepurchase" | "renewal";
  startDate: string;
  durationYears: number;
  expiryDate: string;
  feeHuf: number;
  feeRateId: string | null;
  feeExempt: boolean;
  status: "active" | "superseded" | "cancelled";
  cancelDate: string | null;
  refundHuf: number | null;
  relatedEventId: string | null;
  notes: string;
  voidedAt: string | null;
}

export interface Notice {
  id: string;
  plotId: string;
  leaseId: string | null;
  kind: "letter" | "email" | "label" | "board";
  noticeDate: string;
  result: string;
  voidedAt: string | null;
}

export interface FeeRate {
  id: string;
  cemeteryId: string;
  plotTypeId: string | null;
  feeKind: string;
  amountHuf: number;
  validFrom: string;
  validTo: string | null;
  voidedAt: string | null;
  name: string;
  unit: string;
  vatRate: string;
  ledgerCode: string;
}

export interface Occupant {
  eventId: string;
  folio: number;
  eventType: EventType;
  container: string;
  eventDate: string;
  deceasedId: string;
  deceasedName: string;
  birthDate: string | null;
  deathDate: string | null;
  cryptSlotNo: number | null;
}

export interface PlotDetail {
  plot: Plot;
  plotType: PlotType;
  cemeteryName: string;
  sectionCode: string;
  rowNumber: string;
  status: PlotStatus;
  restPeriodYears: number;
  occupants: Occupant[];
  events: EventRow[];
  leases: Lease[];
  currentLease: Lease | null;
  holders: Holder[];
  restrictions: Restriction[];
  monuments: Monument[];
  notices: Notice[];
  cryptSlots: CryptSlot[];
  resaleAllowedFrom: string | null;
}

export interface Reason {
  code: string;
  params: Record<string, string>;
}
export interface Verdict {
  allowed: boolean;
  blockers: Reason[];
  warnings: Reason[];
}

export interface LeaseProposal {
  kind: string;
  startDate: string;
  durationYears: number;
  expiryDate: string;
  feeHuf: number;
  feeRateId: string | null;
  feeExempt: boolean;
  extensionDays: number;
}

export interface BurialPreview {
  verdict: Verdict;
  status: PlotStatus;
  hasPrimaryHolder: boolean;
  lease: LeaseProposal | null;
  cryptFreeSlots: CryptSlot[];
}

export type PersonRef = { mode: "existing"; id: string } | { mode: "new"; person: Partial<Person> };
export type DeceasedRef = { mode: "existing"; id: string } | { mode: "new"; deceased: Partial<Deceased> };

export interface BurialInput {
  eventType: EventType;
  eventDate: string;
  container: string;
  deceased: DeceasedRef;
  plotId: string | null;
  cryptSlotId: string | null;
  arranger: PersonRef | null;
  holder: PersonRef | null;
  holderRelation: string | null;
  lease: { create: boolean; durationYears?: number | null; feeHuf?: number | null; notes?: string };
  historicalFolio: number | null;
  overrideReason: string | null;
  notes: string;
  cemeteryId?: string | null;
  receiverName?: string;
  receiverAddress?: string;
  destination?: string;
}

export interface BurialResult {
  event: InterEvent;
  folio: number;
  lease: Lease | null;
  verdict: Verdict;
}

export interface ExpiryRow {
  plotId: string;
  identifier: string;
  cemeteryId: string;
  cemeteryName: string;
  sectionCode: string;
  typeName: string;
  status: PlotStatus;
  leaseId: string;
  leaseExpiry: string;
  daysLeft: number;
  holderId: string | null;
  holderName: string | null;
  holderAddress: string | null;
  occupantNames: string | null;
  lastNotice: string | null;
  resaleAllowedFrom: string | null;
}

export interface SearchHit {
  kind: "deceased" | "person" | "plot";
  id: string;
  title: string;
  subtitle: string;
  plotId: string | null;
  plotIdentifier: string | null;
  cemeteryName: string | null;
}

export interface Page<T> {
  items: T[];
  total: number;
}

export interface User {
  id: string;
  username: string;
  displayName: string;
  role: Role;
  extraPermissions: Permission[];
  active: boolean;
  lockedUntil: string | null;
  mustChangePassword: boolean;
  lastLoginAt: string | null;
}

export interface DashboardStats {
  plotsTotal: number;
  byStatus: [PlotStatus, number][];
  eventsThisYear: number;
  expiring90: number;
  expired: number;
  deceasedTotal: number;
  recentEvents: EventRow[];
  daysSinceCertifiedCopy?: number | null;
}

export interface AuditEntry {
  seq: number;
  ts: string;
  userId: string | null;
  userName: string;
  entity: string;
  entityId: string;
  action: string;
  oldValue: string | null;
  newValue: string | null;
  reason: string | null;
  prevHash: string;
  hash: string;
}

export interface PlotFilter {
  cemeteryId?: string | null;
  sectionId?: string | null;
  rowId?: string | null;
  plotTypeId?: string | null;
  statuses?: string[];
  expiryFrom?: string | null;
  expiryTo?: string | null;
  burialFrom?: string | null;
  burialTo?: string | null;
  identifierPrefix?: string | null;
  protectedOnly?: boolean;
  light?: boolean;
  limit?: number | null;
  offset?: number | null;
}

export interface EventFilter {
  cemeteryId?: string | null;
  sectionId?: string | null;
  eventTypes?: string[];
  dateFrom?: string | null;
  dateTo?: string | null;
  folioFrom?: number | null;
  folioTo?: number | null;
  cryptsOnly?: boolean;
  includeVoided?: boolean;
  query?: string | null;
  limit?: number | null;
  offset?: number | null;
  order?: "folio" | "date_desc";
}
