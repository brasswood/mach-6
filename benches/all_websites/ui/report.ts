type SegmentKind =
  | "interning"
  | "indexing"
  | "otherPreprocessing"
  | "distribution"
  | "updatingBloomFilter"
  | "checkingStyleSharing"
  | "queryingSelectorMap"
  | "fastRejecting"
  | "slowRejecting"
  | "slowAccepting"
  | "insertingIntoSharingCache"
  | "other";

type SortDatasetKey = "totalCycles" | "slowRejectCycles";
type CompareSide = "left" | "right";
type ReportSource = "nightly" | "local";

interface ReportJson {
  metadata: ReportMetadataJson;
  websites: WebsiteJson[];
}

interface ReportsIndexJson {
  reports: unknown[];
}

interface ReportMetadataJson {
  branch: string | null;
  commit_hash: string | null;
  dirty: boolean | null;
  report_source?: ReportSource;
  tagline: string | null;
  time_end?: string | null;
}

interface LoadedReport {
  label: string;
  report: ReportJson;
}

interface WebsiteJson {
  website: string;
  summary: SummaryJson;
  selector_slow_rejects_summary: SelectorsSummaryJson;
}

interface SummaryJson {
  baseline: BenchmarkRunSummaryJson;
  fail_caches: BenchmarkRunSummaryJson;
  fail_cache_preprocessing: FailCachePreprocessingSummaryJson;
  preprocessing: PreprocessingSummaryJson;
  after_preprocessing: BenchmarkRunSummaryJson;
}

interface FailCachePreprocessingSummaryJson {
  mean_interning_cycles: number;
}

interface LegacyPreprocessingSummaryJson {
  mean_indexing_cycles: number;
  mean_overall_cycles: number;
}

interface CurrentPreprocessingSummaryJson {
  mean_indexing_cycles: number;
  mean_is_conversion_cycles: number;
  mean_distributing_cycles: number;
}

type PreprocessingSummaryJson = LegacyPreprocessingSummaryJson | CurrentPreprocessingSummaryJson;

interface BenchmarkRunSummaryJson {
  mean_cycles: number;
  counts: CountingStatsJson;
  times: TimingStatsJson;
}

interface CountingStatsJson {
  sharing_instances: number;
  selector_map_hits: number;
  fast_rejects: number;
  slow_rejects: number;
  slow_accepts: number;
}

interface TimingStatsJson {
  means: TimingsJsonBody;
  stddevs: TimingsJsonBody;
}

interface TimingsJsonBody {
  updating_bloom_filter_cycles: number;
  slow_rejecting_cycles: number;
  slow_accepting_cycles: number;
  fast_rejecting_cycles: number;
  checking_style_sharing_cycles: number;
  inserting_into_sharing_cache_cycles: number;
  querying_selector_map_cycles: number;
}

interface SelectorsSummaryJson {
  baseline: SelectorStatsJson;
  fail_caches: SelectorStatsJson;
  after_preprocessing: SelectorStatsJson;
}

interface SelectorStatsJson {
  means_cycles: Record<string, number>;
  stddevs_cycles: Record<string, number>;
}

interface SegmentInfo {
  label: string;
  cssClass: string;
}

interface SelectorRow {
  selector: string;
  meanCycles: bigint;
  stddevCycles: bigint;
}

interface SegmentView {
  kind: SegmentKind;
  meanCycles: bigint;
  stddevCycles: bigint | null;
}

interface BarView {
  label: string;
  segments: SegmentView[];
  totalCycles: bigint;
  totalLengthCycles: bigint;
  slowRejectCycles: bigint;
  counts: CountingStatsJson;
  topSlowRejectSelectors: SelectorRow[];
  showExpandedDetails: boolean;
}

interface ContextBarView {
  label: string;
  totalLengthCycles: bigint;
  aggregateTotalLengthCycles: bigint;
}

interface WebsiteView {
  name: string;
  isAggregate: boolean;
  contextBars: ContextBarView[];
  bars: BarView[];
  summaryMaxBarLengthCycles: bigint;
  totalSortKeyCycles: bigint;
  slowRejectSortKeyCycles: bigint;
  legendKinds: SegmentKind[];
}

interface CompareWebsiteView {
  name: string;
  left: WebsiteView | null;
  right: WebsiteView | null;
}

const MAX_SLOW_REJECT_ROWS = 100;
const NUMBER_FORMAT = new Intl.NumberFormat("en-US");
const REPORT_DATE_FORMAT = new Intl.DateTimeFormat("en-US", {
  weekday: "short",
  month: "short",
  day: "numeric",
  hour: "numeric",
  minute: "2-digit"
});
const SEGMENT_ORDER: readonly SegmentKind[] = [
  "interning",
  "indexing",
  "otherPreprocessing",
  "distribution",
  "updatingBloomFilter",
  "checkingStyleSharing",
  "queryingSelectorMap",
  "fastRejecting",
  "slowRejecting",
  "slowAccepting",
  "insertingIntoSharingCache",
  "other"
];
const SEGMENT_INFO: Record<SegmentKind, SegmentInfo> = {
  interning: { label: "Interning", cssClass: "seg-preprocess-other" },
  indexing: { label: "Indexing", cssClass: "seg-index" },
  otherPreprocessing: { label: "Other :is() Conversion", cssClass: "seg-preprocess-other" },
  distribution: { label: "Distribution", cssClass: "seg-distribution" },
  updatingBloomFilter: { label: "Updating Bloom Filter", cssClass: "seg-bloom" },
  checkingStyleSharing: { label: "Checking Style Sharing", cssClass: "seg-share-check" },
  queryingSelectorMap: { label: "Querying Selector Map", cssClass: "seg-query" },
  fastRejecting: { label: "Fast Rejecting", cssClass: "seg-fast" },
  slowRejecting: { label: "Slow Rejecting", cssClass: "seg-slow" },
  slowAccepting: { label: "Slow Accepting", cssClass: "seg-slow-accept" },
  insertingIntoSharingCache: { label: "Inserting Into Sharing Cache", cssClass: "seg-share-insert" },
  other: { label: "Other", cssClass: "seg-other" }
};

function toBigInt(value: number): bigint {
  return BigInt(value);
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isLegacyPreprocessingSummaryJson(value: unknown): value is LegacyPreprocessingSummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.mean_indexing_cycles)
    && isFiniteNumber(record.mean_overall_cycles)
    && record.mean_is_conversion_cycles === undefined
    && record.mean_distributing_cycles === undefined;
}

function isCurrentPreprocessingSummaryJson(value: unknown): value is CurrentPreprocessingSummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.mean_indexing_cycles)
    && isFiniteNumber(record.mean_is_conversion_cycles)
    && isFiniteNumber(record.mean_distributing_cycles)
    && record.mean_overall_cycles === undefined;
}

function isPreprocessingSummaryJson(value: unknown): value is PreprocessingSummaryJson {
  return isLegacyPreprocessingSummaryJson(value) || isCurrentPreprocessingSummaryJson(value);
}

function getPreprocessingBreakdown(summary: PreprocessingSummaryJson): {
  indexingCycles: bigint;
  otherPreprocessingCycles: bigint;
  distributionCycles: bigint;
} {
  const indexingCycles = toBigInt(summary.mean_indexing_cycles);
  if ("mean_is_conversion_cycles" in summary) {
    const isConversionCycles = toBigInt(summary.mean_is_conversion_cycles);
    return {
      indexingCycles,
      otherPreprocessingCycles: isConversionCycles - indexingCycles,
      distributionCycles: toBigInt(summary.mean_distributing_cycles)
    };
  }

  return {
    indexingCycles,
    otherPreprocessingCycles: toBigInt(summary.mean_overall_cycles) - indexingCycles,
    distributionCycles: 0n
  };
}

function getFailCachePreprocessingBreakdown(summary: FailCachePreprocessingSummaryJson): {
  interningCycles: bigint;
} {
  return {
    interningCycles: toBigInt(summary.mean_interning_cycles)
  };
}

function formatCycles(cycles: bigint): string {
  const value = Number(cycles);
  if (value >= 1_000_000_000_000) {
    return (value / 1_000_000_000_000).toFixed(3) + "T cycles";
  }
  if (value >= 1_000_000_000) {
    return (value / 1_000_000_000).toFixed(3) + "B cycles";
  }
  if (value >= 1_000_000) {
    return (value / 1_000_000).toFixed(3) + "M cycles";
  }
  if (value >= 1_000) {
    return (value / 1_000).toFixed(3) + "K cycles";
  }
  return value.toString() + " cycles";
}

function formatSignedCycles(cycles: bigint): string {
  return cycles < 0n ? "-" + formatCycles(-cycles) : formatCycles(cycles);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function meanWithStddev(meanCycles: bigint, stddevCycles: bigint): string {
  return formatCycles(meanCycles) + " \u00B1 " + formatCycles(stddevCycles);
}

function buildPageTitle(metadata: ReportMetadataJson): string {
  const parts: string[] = [];
  if (metadata.branch) {
    parts.push("\u2387 " + metadata.branch);
  }
  if (metadata.commit_hash) {
    let commit = metadata.commit_hash.slice(0, 7);
    if (metadata.dirty) {
      commit += "-dirty";
    }
    parts.push(commit);
  }
  parts.push("Mach 6 Report");
  return parts.join(" • ");
}

function renderMetadata(metadata: ReportMetadataJson, commitLine: HTMLElement): void {
  document.title = buildPageTitle(metadata);

  if (!metadata.commit_hash) {
    commitLine.hidden = true;
    commitLine.innerHTML = "";
    return;
  }

  const commitHashShort = metadata.commit_hash.slice(0, 7);
  const branchHtml = metadata.branch
    ? '<span>\u2387 ' + escapeHtml(metadata.branch) + '</span><span>•</span>'
    : "";
  const dirtyHtml = metadata.dirty
    ? '-<span class="commit-pill-dirty">dirty</span>'
    : "";
  const taglineHtml = metadata.tagline
    ? '<span class="commit-tagline">' + escapeHtml(metadata.tagline) + '</span>'
    : "";

  commitLine.innerHTML = [
    '<span class="commit-pill">',
    branchHtml,
    '<span>' + escapeHtml(commitHashShort) + dirtyHtml + '</span>',
    '</span>',
    taglineHtml
  ].join("");
  commitLine.hidden = false;
}

function isReportsIndexJson(value: unknown): value is ReportsIndexJson {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Array.isArray(record.reports);
}

function isReportSource(value: unknown): value is ReportSource {
  return value === "nightly" || value === "local";
}

function getRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null) {
    return null;
  }
  return value as Record<string, unknown>;
}

function getOptionalString(record: Record<string, unknown>, key: string): string | null {
  const value = record[key];
  return typeof value === "string" ? value : null;
}

function getOptionalBoolean(record: Record<string, unknown>, key: string): boolean | null {
  const value = record[key];
  return typeof value === "boolean" ? value : null;
}

function normalizeReportUrl(url: string): string {
  return url.endsWith("/") ? url : url + "/";
}

function currentReportUrl(): string {
  const url = new URL(window.location.href);
  let pathname = url.pathname;
  if (pathname.endsWith("/index.html")) {
    pathname = pathname.slice(0, -"/index.html".length);
  }
  return normalizeReportUrl(pathname);
}

function formatReportDate(timeEnd: string | null): string | null {
  if (!timeEnd) {
    return null;
  }
  const parsed = new Date(timeEnd);
  if (Number.isNaN(parsed.getTime())) {
    return null;
  }
  return REPORT_DATE_FORMAT.format(parsed);
}

function getReportSource(metadata: ReportMetadataJson): ReportSource {
  return metadata.report_source ?? "local";
}

function buildCurrentReportLabel(metadata: ReportMetadataJson): string {
  return buildReportOptionLabel({
    url: currentReportUrl(),
    metadata
  });
}

function buildReportOptionLabel(entry: Record<string, unknown>): string {
  const metadata = getRecord(entry.metadata) ?? getRecord(entry.raw_metadata);
  const fallbackUrl = getOptionalString(entry, "url") ?? "Unknown report";
  if (metadata === null) {
    return fallbackUrl;
  }

  const commitHash = getOptionalString(metadata, "commit_hash");
  const shortCommit = commitHash ? commitHash.slice(0, 7) : null;
  const dirtyCommit = shortCommit
    ? shortCommit + (getOptionalBoolean(metadata, "dirty") ? "-dirty" : "")
    : null;
  const tagline = getOptionalString(metadata, "tagline");
  const branch = getOptionalString(metadata, "branch");
  const formattedDate = formatReportDate(getOptionalString(metadata, "time_end"));

  if (dirtyCommit) {
    let label = dirtyCommit;
    if (tagline) {
      label += ": " + tagline;
    }
    if (branch) {
      label += " (" + branch + ")";
    }
    if (formattedDate) {
      label += " (" + formattedDate + ")";
    }
    return label;
  }

  if (tagline) {
    let label = tagline;
    if (branch) {
      label += " (" + branch + ")";
    }
    if (formattedDate) {
      label += " (" + formattedDate + ")";
    }
    return label;
  }

  const suffixes: string[] = [];
  if (branch) {
    suffixes.push("(" + branch + ")");
  }
  if (formattedDate) {
    suffixes.push("(" + formattedDate + ")");
  }
  return suffixes.length > 0 ? suffixes.join(" ") : fallbackUrl;
}

function populateCompareSelect(
  select: HTMLSelectElement,
  reportEntries: Record<string, unknown>[],
  selectedUrl: string
): void {
  select.innerHTML = "";
  for (const entry of reportEntries) {
    const url = getOptionalString(entry, "url");
    if (!url) {
      continue;
    }
    const option = document.createElement("option");
    option.value = url;
    option.textContent = buildReportOptionLabel(entry);
    option.selected = normalizeReportUrl(url) === normalizeReportUrl(selectedUrl);
    select.appendChild(option);
  }
}

function setCompareModeVisibility(
  reportSource: ReportSource,
  leftSelectField: HTMLElement,
  rightSelectField: HTMLElement,
  leftFileField: HTMLElement,
  rightFileField: HTMLElement
): void {
  const useNightly = reportSource === "nightly";
  leftSelectField.hidden = !useNightly;
  rightSelectField.hidden = !useNightly;
  leftFileField.hidden = useNightly;
  rightFileField.hidden = useNightly;
}

async function loadCompareControls(
  container: HTMLElement,
  leftSelectField: HTMLElement,
  rightSelectField: HTMLElement,
  leftFileField: HTMLElement,
  rightFileField: HTMLElement,
  leftSelect: HTMLSelectElement,
  rightSelect: HTMLSelectElement,
  leftFileInput: HTMLInputElement,
  rightFileInput: HTMLInputElement,
  compareStatus: HTMLElement,
  currentMetadata: ReportMetadataJson
): Promise<void> {
  const reportSource = getReportSource(currentMetadata);
  setCompareModeVisibility(reportSource, leftSelectField, rightSelectField, leftFileField, rightFileField);
  if (reportSource === "local") {
    leftSelect.innerHTML = "";
    rightSelect.innerHTML = "";
    leftFileInput.value = "";
    rightFileInput.value = "";
    container.hidden = false;
    compareStatus.hidden = false;
    compareStatus.classList.remove("error");
    compareStatus.textContent = "Choose local report.json files or leave a side empty to use the current report.";
    return;
  }
  try {
    const response = await fetch("reports-index.json");
    if (!response.ok) {
      throw new Error("HTTP " + response.status + " while loading reports-index.json");
    }

    const raw: unknown = await response.json();
    if (!isReportsIndexJson(raw)) {
      throw new Error("reports-index.json had an unexpected shape");
    }

    const reportEntries = raw.reports.flatMap((entry) => {
      const record = getRecord(entry);
      return record === null ? [] : [record];
    });
    if (reportEntries.length === 0) {
      throw new Error("reports-index.json did not contain any reports");
    }

    const currentUrl = currentReportUrl();
    reportEntries.unshift({
      url: currentUrl,
      metadata: currentMetadata
    });

    populateCompareSelect(leftSelect, reportEntries, currentUrl);
    populateCompareSelect(rightSelect, reportEntries, currentUrl);

    container.hidden = false;
    compareStatus.hidden = true;
    compareStatus.textContent = "";
    compareStatus.classList.remove("error");
  } catch (error: unknown) {
    container.hidden = false;
    leftSelect.innerHTML = '<option selected>Compare unavailable</option>';
    rightSelect.innerHTML = '<option selected>Compare unavailable</option>';
    compareStatus.hidden = false;
    compareStatus.classList.add("error");
    const message = error instanceof Error ? error.message : String(error);
    compareStatus.textContent = "Failed to load available reports: " + message;
  }
}

function joinReportPath(reportUrl: string, fileName: string): string {
  const normalizedUrl = normalizeReportUrl(reportUrl);
  return normalizedUrl + fileName;
}

async function fetchReportJson(reportUrl: string): Promise<ReportJson> {
  const response = await fetch(joinReportPath(reportUrl, "report.json"));
  if (!response.ok) {
    throw new Error("HTTP " + response.status + " while loading " + joinReportPath(reportUrl, "report.json"));
  }

  const raw: unknown = await response.json();
  if (!isReportJson(raw)) {
    throw new Error(joinReportPath(reportUrl, "report.json") + " had an unexpected shape");
  }
  return raw;
}

async function readReportJsonFile(file: File): Promise<ReportJson> {
  const raw: unknown = JSON.parse(await file.text());
  if (!isReportJson(raw)) {
    throw new Error(file.name + " had an unexpected shape");
  }
  return raw;
}

async function resolveFileSelection(
  input: HTMLInputElement,
  fallbackReport: ReportJson,
  fallbackLabel: string
): Promise<LoadedReport> {
  const file = input.files?.[0];
  if (!file) {
    return {
      label: fallbackLabel,
      report: fallbackReport
    };
  }
  return {
    label: file.name,
    report: await readReportJsonFile(file)
  };
}

function setCompareStatus(compareStatus: HTMLElement, message: string, isError: boolean): void {
  compareStatus.hidden = false;
  compareStatus.classList.toggle("error", isError);
  compareStatus.textContent = message;
}

function formatWebsiteCount(count: number): string {
  return count.toString() + " " + (count === 1 ? "website" : "websites");
}

function renderSingleReportList(
  list: HTMLElement,
  report: ReportJson,
  label: string | null
): void {
  const websites = buildReportWebsiteViews(report);
  const headerHtml = label === null
    ? ""
    : '<h3 class="compare-column-header">' + renderCompareHeaderHtml(report.metadata, label) + '</h3>';

  list.innerHTML = [
    headerHtml,
    websites.map((website) => {
      return renderWebsite(website);
    }).join(""),
  ].join("");
}

function renderDefaultReportList(
  list: HTMLElement,
  byTotal: HTMLButtonElement,
  bySlow: HTMLButtonElement,
  report: ReportJson
): void {
  renderSingleReportList(list, report, null);
  sortBy("totalCycles", byTotal, list, byTotal, bySlow);
}

function hideCompareResults(compareResults: HTMLElement): void {
  compareResults.innerHTML = "";
  compareResults.hidden = true;
}

function installCompareHandler(
  currentReport: ReportJson,
  compareButton: HTMLButtonElement,
  leftOnlyButton: HTMLButtonElement,
  rightOnlyButton: HTMLButtonElement,
  reportSource: ReportSource,
  leftSelect: HTMLSelectElement,
  rightSelect: HTMLSelectElement,
  leftFileInput: HTMLInputElement,
  rightFileInput: HTMLInputElement,
  compareStatus: HTMLElement,
  list: HTMLElement,
  sortControls: HTMLElement,
  compareResults: HTMLElement,
  byTotal: HTMLButtonElement,
  bySlow: HTMLButtonElement
): void {
  const setButtonsDisabled = (disabled: boolean): void => {
    compareButton.disabled = disabled;
    leftOnlyButton.disabled = disabled;
    rightOnlyButton.disabled = disabled;
  };

  const currentLabel = buildCurrentReportLabel(currentReport.metadata);

  const loadSelectedReport = async (side: CompareSide): Promise<LoadedReport> => {
    if (reportSource === "nightly") {
      const select = side === "left" ? leftSelect : rightSelect;
      const report = await fetchReportJson(select.value);
      return {
        label: select.selectedOptions[0]?.textContent ?? (side === "left" ? "Left report" : "Right report"),
        report
      };
    }
    const input = side === "left" ? leftFileInput : rightFileInput;
    return resolveFileSelection(input, currentReport, currentLabel);
  };

  const renderTwoSided = async (): Promise<void> => {
    const [leftLoadedReport, rightLoadedReport] = await Promise.all([
      loadSelectedReport("left"),
      loadSelectedReport("right")
    ]);
    renderCompareResults(
      compareResults,
      leftLoadedReport.report,
      rightLoadedReport.report,
      leftLoadedReport.label,
      rightLoadedReport.label
    );
    installCompareSortHandlers(compareResults);
    const defaultSortButton = compareResults.querySelector<HTMLButtonElement>(
      '.compare-sort-controls button[data-compare-side="left"][data-sort-key="totalCycles"]'
    );
    if (defaultSortButton) {
      sortCompareRows(compareResults, "left", "totalCycles", defaultSortButton);
    }
    syncCompareDetails(compareResults);
    list.hidden = true;
    sortControls.hidden = true;
    document.body.classList.add("compare-active");
    setCompareStatus(
      compareStatus,
      "Showing compare view for " + formatWebsiteCount(leftLoadedReport.report.websites.length) + " on the left and "
        + formatWebsiteCount(rightLoadedReport.report.websites.length) + " on the right.",
      false
    );
  };

  const renderOneSided = async (side: "left" | "right"): Promise<void> => {
    const loadedReport = await loadSelectedReport(side);
    renderSingleReportList(list, loadedReport.report, loadedReport.label);
    hideCompareResults(compareResults);
    list.hidden = false;
    sortControls.hidden = false;
    document.body.classList.remove("compare-active");
    sortBy("totalCycles", byTotal, list, byTotal, bySlow);
    setCompareStatus(
      compareStatus,
      "Showing " + (side === "left" ? "left" : "right") + " report only for "
        + formatWebsiteCount(loadedReport.report.websites.length) + ".",
      false
    );
  };

  const installAction = (
    button: HTMLButtonElement,
    loadingMessage: string,
    action: () => Promise<void>
  ): void => {
    button.addEventListener("click", async () => {
      setButtonsDisabled(true);
      setCompareStatus(compareStatus, loadingMessage, false);

      try {
        await action();
      } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        hideCompareResults(compareResults);
        document.body.classList.remove("compare-active");
        setCompareStatus(compareStatus, "Failed to load compare reports: " + message, true);
      } finally {
        setButtonsDisabled(false);
      }
    });
  };

  installAction(compareButton, "Loading selected reports...", renderTwoSided);
  installAction(leftOnlyButton, "Loading left report...", async () => {
    await renderOneSided("left");
  });
  installAction(rightOnlyButton, "Loading right report...", async () => {
    await renderOneSided("right");
  });
}

function buildSelectorRows(stats: SelectorStatsJson): SelectorRow[] {
  const rows = Object.entries(stats.means_cycles).map(([selector, meanCycles]) => {
    const stddevCycles = stats.stddevs_cycles[selector];
    if (stddevCycles === undefined) {
      throw new Error("Missing stddev for selector: " + selector);
    }
    return {
      selector,
      meanCycles: toBigInt(meanCycles),
      stddevCycles: toBigInt(stddevCycles)
    };
  });
  rows.sort((left, right) => {
    if (left.meanCycles === right.meanCycles) {
      return left.selector.localeCompare(right.selector);
    }
    return left.meanCycles > right.meanCycles ? -1 : 1;
  });
  return rows.slice(0, MAX_SLOW_REJECT_ROWS);
}

function buildBar(
  label: string,
  summary: BenchmarkRunSummaryJson,
  selectorsSummary: SelectorStatsJson,
  includeFailCachePreprocessing: FailCachePreprocessingSummaryJson | null,
  includePreprocessing: PreprocessingSummaryJson | null,
  showExpandedDetails = true
): BarView {
  const means = summary.times.means;
  const stddevs = summary.times.stddevs;
  const measuredMatchDurations: SegmentView[] = [
    { kind: "updatingBloomFilter", meanCycles: toBigInt(means.updating_bloom_filter_cycles), stddevCycles: toBigInt(stddevs.updating_bloom_filter_cycles) },
    { kind: "checkingStyleSharing", meanCycles: toBigInt(means.checking_style_sharing_cycles), stddevCycles: toBigInt(stddevs.checking_style_sharing_cycles) },
    { kind: "queryingSelectorMap", meanCycles: toBigInt(means.querying_selector_map_cycles), stddevCycles: toBigInt(stddevs.querying_selector_map_cycles) },
    { kind: "fastRejecting", meanCycles: toBigInt(means.fast_rejecting_cycles), stddevCycles: toBigInt(stddevs.fast_rejecting_cycles) },
    { kind: "slowRejecting", meanCycles: toBigInt(means.slow_rejecting_cycles), stddevCycles: toBigInt(stddevs.slow_rejecting_cycles) },
    { kind: "slowAccepting", meanCycles: toBigInt(means.slow_accepting_cycles), stddevCycles: toBigInt(stddevs.slow_accepting_cycles) },
    { kind: "insertingIntoSharingCache", meanCycles: toBigInt(means.inserting_into_sharing_cache_cycles), stddevCycles: toBigInt(stddevs.inserting_into_sharing_cache_cycles) }
  ];
  const measuredMatchSum = measuredMatchDurations.reduce((sum, segment) => {
    return sum + segment.meanCycles;
  }, 0n);
  measuredMatchDurations.push({
    kind: "other",
    meanCycles: toBigInt(summary.mean_cycles) - measuredMatchSum,
    stddevCycles: null
  });

  const segments: SegmentView[] = [];
  if (includeFailCachePreprocessing) {
    const { interningCycles } = getFailCachePreprocessingBreakdown(includeFailCachePreprocessing);
    segments.push({ kind: "interning", meanCycles: interningCycles, stddevCycles: null });
  }
  if (includePreprocessing) {
    const {
      indexingCycles,
      otherPreprocessingCycles,
      distributionCycles
    } = getPreprocessingBreakdown(includePreprocessing);
    segments.push({ kind: "indexing", meanCycles: indexingCycles, stddevCycles: null });
    segments.push({ kind: "otherPreprocessing", meanCycles: otherPreprocessingCycles, stddevCycles: null });
    segments.push({ kind: "distribution", meanCycles: distributionCycles, stddevCycles: null });
  }
  segments.push(...measuredMatchDurations);

  const totalCycles = segments.reduce((sum, segment) => {
    return sum + segment.meanCycles;
  }, 0n);
  const totalLengthCycles = segments.reduce((sum, segment) => {
    return sum + (segment.meanCycles > 0n ? segment.meanCycles : 0n);
  }, 0n);
  const slowRejectSegment = segments.find((segment) => segment.kind === "slowRejecting");
  if (!slowRejectSegment) {
    throw new Error("Missing slow-reject segment for " + label);
  }

  return {
    label,
    segments,
    totalCycles,
    totalLengthCycles,
    slowRejectCycles: slowRejectSegment.meanCycles,
    counts: summary.counts,
    topSlowRejectSelectors: buildSelectorRows(selectorsSummary),
    showExpandedDetails
  };
}

function buildWebsiteBars(website: WebsiteJson): [BarView, BarView, BarView, BarView, BarView] {
  return [
    buildBar("Baseline", website.summary.baseline, website.selector_slow_rejects_summary.baseline, null, null),
    buildBar("Fail Caches", website.summary.fail_caches, website.selector_slow_rejects_summary.fail_caches, null, null, false),
    buildBar("Interning + Fail Caches", website.summary.fail_caches, website.selector_slow_rejects_summary.fail_caches, website.summary.fail_cache_preprocessing, null),
    buildBar("After Preprocessing", website.summary.after_preprocessing, website.selector_slow_rejects_summary.after_preprocessing, null, null, false),
    buildBar("With Preprocessing", website.summary.after_preprocessing, website.selector_slow_rejects_summary.after_preprocessing, null, website.summary.preprocessing)
  ];
}

function buildWebsiteView(
  website: WebsiteJson,
  aggregateBars: readonly BarView[],
  isAggregate = false
): WebsiteView {
  const bars = buildWebsiteBars(website);
  const contextBars = bars.map((bar, index) => {
    const aggregateBar = aggregateBars[index];
    if (!aggregateBar) {
      throw new Error("Missing aggregate bar for " + bar.label);
    }
    return {
      label: bar.label,
      totalLengthCycles: bar.totalLengthCycles,
      aggregateTotalLengthCycles: aggregateBar.totalLengthCycles
    };
  });
  const summaryMaxBarLengthCycles = bars.reduce((max, bar) => {
    return bar.totalLengthCycles > max ? bar.totalLengthCycles : max;
  }, 0n);

  const totalSortKeyCycles = bars.reduce((max, bar) => {
    return bar.totalCycles > max ? bar.totalCycles : max;
  }, 0n);
  const slowRejectSortKeyCycles = bars.reduce((max, bar) => {
    return bar.slowRejectCycles > max ? bar.slowRejectCycles : max;
  }, 0n);

  const legendKinds = SEGMENT_ORDER.filter((kind) => {
    return bars.some((bar) => {
      return bar.segments.some((segment) => segment.kind === kind);
    });
  });

  return {
    name: website.website,
    isAggregate,
    contextBars,
    bars,
    summaryMaxBarLengthCycles,
    totalSortKeyCycles,
    slowRejectSortKeyCycles,
    legendKinds: [...legendKinds]
  };
}

function sumNumbers(values: number[]): number {
  return values.reduce((sum, value) => {
    return sum + value;
  }, 0);
}

function combineStddevs(values: number[]): number {
  return Math.round(Math.sqrt(values.reduce((sum, value) => {
    return sum + (value * value);
  }, 0)));
}

function sumRecordValues(records: Record<string, number>[]): Record<string, number> {
  const summed: Record<string, number> = {};
  for (const record of records) {
    for (const [key, value] of Object.entries(record)) {
      summed[key] = (summed[key] ?? 0) + value;
    }
  }
  return summed;
}

function aggregateBenchmarkRunSummary(summaries: BenchmarkRunSummaryJson[]): BenchmarkRunSummaryJson {
  return {
    mean_cycles: sumNumbers(summaries.map((summary) => summary.mean_cycles)),
    counts: {
      sharing_instances: sumNumbers(summaries.map((summary) => summary.counts.sharing_instances)),
      selector_map_hits: sumNumbers(summaries.map((summary) => summary.counts.selector_map_hits)),
      fast_rejects: sumNumbers(summaries.map((summary) => summary.counts.fast_rejects)),
      slow_rejects: sumNumbers(summaries.map((summary) => summary.counts.slow_rejects)),
      slow_accepts: sumNumbers(summaries.map((summary) => summary.counts.slow_accepts))
    },
    times: {
      means: {
        updating_bloom_filter_cycles: sumNumbers(summaries.map((summary) => summary.times.means.updating_bloom_filter_cycles)),
        slow_rejecting_cycles: sumNumbers(summaries.map((summary) => summary.times.means.slow_rejecting_cycles)),
        slow_accepting_cycles: sumNumbers(summaries.map((summary) => summary.times.means.slow_accepting_cycles)),
        fast_rejecting_cycles: sumNumbers(summaries.map((summary) => summary.times.means.fast_rejecting_cycles)),
        checking_style_sharing_cycles: sumNumbers(summaries.map((summary) => summary.times.means.checking_style_sharing_cycles)),
        inserting_into_sharing_cache_cycles: sumNumbers(summaries.map((summary) => summary.times.means.inserting_into_sharing_cache_cycles)),
        querying_selector_map_cycles: sumNumbers(summaries.map((summary) => summary.times.means.querying_selector_map_cycles))
      },
      stddevs: {
        updating_bloom_filter_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.updating_bloom_filter_cycles)),
        slow_rejecting_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.slow_rejecting_cycles)),
        slow_accepting_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.slow_accepting_cycles)),
        fast_rejecting_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.fast_rejecting_cycles)),
        checking_style_sharing_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.checking_style_sharing_cycles)),
        inserting_into_sharing_cache_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.inserting_into_sharing_cache_cycles)),
        querying_selector_map_cycles: combineStddevs(summaries.map((summary) => summary.times.stddevs.querying_selector_map_cycles))
      }
    }
  };
}

function aggregateSelectorStats(stats: SelectorStatsJson[]): SelectorStatsJson {
  const selectorKeys = new Set<string>();
  for (const entry of stats) {
    for (const key of Object.keys(entry.stddevs_cycles)) {
      selectorKeys.add(key);
    }
  }

  const stddevsCycles: Record<string, number> = {};
  for (const key of selectorKeys) {
    stddevsCycles[key] = combineStddevs(stats.map((entry) => entry.stddevs_cycles[key] ?? 0));
  }

  return {
    means_cycles: sumRecordValues(stats.map((entry) => entry.means_cycles)),
    stddevs_cycles: stddevsCycles
  };
}

function buildAggregateWebsiteJson(websites: WebsiteJson[]): WebsiteJson {
  const preprocessingBreakdowns = websites.map((website) => {
    return getPreprocessingBreakdown(website.summary.preprocessing);
  });
  const failCachePreprocessingBreakdowns = websites.map((website) => {
    return getFailCachePreprocessingBreakdown(website.summary.fail_cache_preprocessing);
  });
  return {
    website: "All Websites (" + websites.length.toString() + ")",
    summary: {
      baseline: aggregateBenchmarkRunSummary(websites.map((website) => website.summary.baseline)),
      fail_caches: aggregateBenchmarkRunSummary(websites.map((website) => website.summary.fail_caches)),
      fail_cache_preprocessing: {
        mean_interning_cycles: Number(failCachePreprocessingBreakdowns.reduce((sum, breakdown) => {
          return sum + breakdown.interningCycles;
        }, 0n))
      },
      preprocessing: {
        mean_indexing_cycles: Number(preprocessingBreakdowns.reduce((sum, breakdown) => {
          return sum + breakdown.indexingCycles;
        }, 0n)),
        mean_is_conversion_cycles: Number(preprocessingBreakdowns.reduce((sum, breakdown) => {
          return sum + breakdown.indexingCycles + breakdown.otherPreprocessingCycles;
        }, 0n)),
        mean_distributing_cycles: Number(preprocessingBreakdowns.reduce((sum, breakdown) => {
          return sum + breakdown.distributionCycles;
        }, 0n))
      },
      after_preprocessing: aggregateBenchmarkRunSummary(websites.map((website) => website.summary.after_preprocessing))
    },
    selector_slow_rejects_summary: {
      baseline: aggregateSelectorStats(websites.map((website) => website.selector_slow_rejects_summary.baseline)),
      fail_caches: aggregateSelectorStats(websites.map((website) => website.selector_slow_rejects_summary.fail_caches)),
      after_preprocessing: aggregateSelectorStats(websites.map((website) => website.selector_slow_rejects_summary.after_preprocessing))
    }
  };
}

function buildReportWebsiteViews(report: ReportJson): WebsiteView[] {
  const aggregateWebsite = buildAggregateWebsiteJson(report.websites);
  const aggregateWebsiteBars = buildWebsiteBars(aggregateWebsite);
  const aggregateWebsiteView = buildWebsiteView(aggregateWebsite, aggregateWebsiteBars, true);
  return [
    aggregateWebsiteView,
    ...report.websites.map((website) => buildWebsiteView(website, aggregateWebsiteView.bars))
  ];
}

function pct(numerator: bigint, denominator: bigint): string {
  if (denominator === 0n) {
    return "0.00";
  }
  return ((Number(numerator) / Number(denominator)) * 100).toFixed(2);
}

function renderSegmentSwatch(kind: SegmentKind): string {
  const info = SEGMENT_INFO[kind];
  return '<i class="swatch ' + info.cssClass + '"></i>' + escapeHtml(info.label);
}

function renderContextBar(bar: ContextBarView): string {
  return [
    '<div class="variant-context">',
    '<div class="variant-context-label">' + escapeHtml(bar.label) + '</div>',
    '<div class="context-bar-wrap"><div class="context-bar-total" style="width: ' + pct(bar.totalLengthCycles, bar.aggregateTotalLengthCycles) + '%"></div></div>',
    '<div class="context-value">' + escapeHtml(pct(bar.totalLengthCycles, bar.aggregateTotalLengthCycles)) + '%</div>',
    '</div>'
  ].join("");
}

function renderSummaryBar(bar: BarView, summaryMaxBarLengthCycles: bigint): string {
  const segmentsHtml = bar.segments.map((segment) => {
    return '<div class="bar-seg ' + SEGMENT_INFO[segment.kind].cssClass + '" style="width: ' + pct(segment.meanCycles > 0n ? segment.meanCycles : 0n, bar.totalLengthCycles) + '%"></div>';
  }).join("");
  const warningClass = bar.totalLengthCycles !== bar.totalCycles ? " warning" : "";
  const displayNote = bar.totalLengthCycles !== bar.totalCycles
    ? '<div class="time-display-note">Displayed: ' + escapeHtml(formatCycles(bar.totalLengthCycles)) + '</div>'
    : "";

  return [
    '<div class="variant-summary">',
    '<div class="variant-label">' + escapeHtml(bar.label) + '</div>',
    '<div class="bar-wrap"><div class="bar-total" style="width: ' + pct(bar.totalLengthCycles, summaryMaxBarLengthCycles) + '%">' + segmentsHtml + '</div></div>',
    '<div class="time"><div class="time-value' + warningClass + '">' + escapeHtml(formatCycles(bar.totalCycles)) + '</div>' + displayNote + '</div>',
    '</div>'
  ].join("");
}

function renderExpandedBar(bar: BarView): string {
  const segmentsHtml = bar.segments.map((segment) => {
    return '<div class="expanded-bar-seg ' + SEGMENT_INFO[segment.kind].cssClass + '" style="width: ' + pct(segment.meanCycles > 0n ? segment.meanCycles : 0n, bar.totalLengthCycles) + '%"></div>';
  }).join("");
  const legendHtml = bar.segments.map((segment) => {
    const warningClass = segment.meanCycles < 0n ? "legend-warning" : "";
    const value = segment.stddevCycles === null
      ? formatSignedCycles(segment.meanCycles)
      : formatSignedCycles(segment.meanCycles) + " \u00B1 " + formatCycles(segment.stddevCycles);
    return '<span class="' + warningClass + '">' + renderSegmentSwatch(segment.kind) + ': ' + escapeHtml(value) + '</span>';
  }).join("") + '<span>Total: ' + escapeHtml(formatCycles(bar.totalCycles)) + '</span>';

  return [
    '<section class="expanded-chart">',
    '<h5>Cycle Breakdown</h5>',
    '<div class="expanded-bar-wrap"><div class="expanded-bar-total">' + segmentsHtml + '</div></div>',
    '<div class="expanded-legend">' + legendHtml + '</div>',
    '</section>'
  ].join("");
}

function renderSelectorRows(rows: SelectorRow[]): string {
  if (rows.length === 0) {
    return '<tr><td colspan="2">No selector stats captured.</td></tr>';
  }
  return rows.map((row) => {
    return [
      '<tr>',
      '<td class="col-selector"><div class="cell-scroll"><code>' + escapeHtml(row.selector) + '</code></div></td>',
      '<td class="col-time"><div class="cell-scroll">' + escapeHtml(meanWithStddev(row.meanCycles, row.stddevCycles)) + '</div></td>',
      '</tr>'
    ].join("");
  }).join("");
}

function renderVariantDetails(bar: BarView): string {
  return [
    '<section class="variant-details">',
    '<h4 class="variant-details-title">' + escapeHtml(bar.label) + '</h4>',
    renderExpandedBar(bar),
    '<table><tbody>',
    '<tr><th>Sharing Instances</th><td>' + escapeHtml(NUMBER_FORMAT.format(bar.counts.sharing_instances)) + '</td></tr>',
    '<tr><th>Selector Map Hits</th><td>' + escapeHtml(NUMBER_FORMAT.format(bar.counts.selector_map_hits)) + '</td></tr>',
    '<tr><th>Fast Rejects</th><td>' + escapeHtml(NUMBER_FORMAT.format(bar.counts.fast_rejects)) + '</td></tr>',
    '<tr><th>Slow Rejects</th><td>' + escapeHtml(NUMBER_FORMAT.format(bar.counts.slow_rejects)) + '</td></tr>',
    '<tr><th>Slow Accepts</th><td>' + escapeHtml(NUMBER_FORMAT.format(bar.counts.slow_accepts)) + '</td></tr>',
    '</tbody></table>',
    '<details class="selector-breakdown">',
    '<summary>Slow-Reject Cycles Aggregated by Selector (Top ' + MAX_SLOW_REJECT_ROWS + ')</summary>',
    '<div class="selector-breakdown-inner">',
    '<table class="selector-breakdown-table">',
    '<thead><tr><th class="col-selector">Selector</th><th class="col-time">Total Slow Reject Cycles</th></tr></thead>',
    '<tbody>' + renderSelectorRows(bar.topSlowRejectSelectors) + '</tbody>',
    '</table>',
    '</div>',
    '</details>',
    '</section>'
  ].join("");
}

function renderWebsite(website: WebsiteView): string {
  return [
    '<details class="site" data-total-cycles="' + website.totalSortKeyCycles.toString() + '" data-slow-reject-cycles="' + website.slowRejectSortKeyCycles.toString() + '">',
    '<summary>',
    '<div class="row">',
    '<div class="chevron" aria-hidden="true"></div>',
    '<div class="name">' + (website.isAggregate
      ? '<span class="aggregate-label">' + escapeHtml(website.name) + '</span>'
      : escapeHtml(website.name)) + '</div>',
    '<div class="summary-variants">' + website.contextBars.map((bar) => {
      return renderContextBar(bar);
    }).join("") + website.bars.map((bar) => {
      return renderSummaryBar(bar, website.summaryMaxBarLengthCycles);
    }).join("") + '</div>',
    '</div>',
    '<div class="bar-legend">' + website.legendKinds.map((kind) => {
      return '<span>' + renderSegmentSwatch(kind) + '</span>';
    }).join("") + '</div>',
    '</summary>',
    '<div class="details">',
    '<div class="details-variants">' + website.bars.filter((bar) => bar.showExpandedDetails).map(renderVariantDetails).join("") + '</div>',
    '</div>',
    '</details>'
  ].join("");
}

function buildWebsiteMap(websites: WebsiteJson[]): Map<string, WebsiteView> {
  const websiteMap = new Map<string, WebsiteView>();
  const aggregateWebsite = buildAggregateWebsiteJson(websites);
  const aggregateWebsiteBars = buildWebsiteBars(aggregateWebsite);
  const aggregateWebsiteView = buildWebsiteView(aggregateWebsite, aggregateWebsiteBars, true);
  websiteMap.set(aggregateWebsite.website, aggregateWebsiteView);
  for (const website of websites) {
    websiteMap.set(website.website, buildWebsiteView(website, aggregateWebsiteView.bars));
  }
  return websiteMap;
}

function buildCompareWebsites(leftReport: ReportJson, rightReport: ReportJson): CompareWebsiteView[] {
  const leftMap = buildWebsiteMap(leftReport.websites);
  const rightMap = buildWebsiteMap(rightReport.websites);
  const names = new Set<string>();
  for (const name of leftMap.keys()) {
    names.add(name);
  }
  for (const name of rightMap.keys()) {
    names.add(name);
  }

  return Array.from(names).sort((left, right) => {
    return left.localeCompare(right);
  }).map((name) => {
    return {
      name,
      left: leftMap.get(name) ?? null,
      right: rightMap.get(name) ?? null
    };
  });
}

function renderCompareCell(
  website: WebsiteView | null,
  missingLabel: string
): string {
  if (website === null) {
    return '<p class="compare-empty">' + escapeHtml(missingLabel) + '</p>';
  }
  return renderWebsite(website);
}

function renderCompareHeaderHtml(metadata: ReportMetadataJson, fallbackLabel: string): string {
  const parts: string[] = [];
  const commitHash = metadata.commit_hash;
  if (commitHash) {
    let commitLabel = commitHash.slice(0, 7);
    if (metadata.dirty) {
      commitLabel += "-dirty";
    }
    parts.push('<span class="compare-header-commit">' + escapeHtml(commitLabel) + "</span>");
  }
  if (metadata.tagline) {
    if (commitHash) {
      parts.push(": " + escapeHtml(metadata.tagline));
    } else {
      parts.push(escapeHtml(metadata.tagline));
    }
  }
  if (metadata.branch) {
    parts.push(' (<span class="compare-header-branch">' + escapeHtml(metadata.branch) + "</span>)");
  }

  return parts.length > 0 ? parts.join("") : escapeHtml(fallbackLabel);
}

function renderCompareSortControls(side: CompareSide): string {
  const sideLabel = side === "left" ? "left" : "right";
  return [
    '<div class="compare-sort-controls" role="group" aria-label="Sort websites by ' + sideLabel + ' report">',
    '<button class="sort-btn active" type="button" data-compare-side="' + side + '" data-sort-key="totalCycles">Sort by Overall Cycles</button>',
    '<button class="sort-btn" type="button" data-compare-side="' + side + '" data-sort-key="slowRejectCycles">Sort by Slow-Reject Cycles</button>',
    '</div>'
  ].join("");
}

function renderCompareResults(
  compareResults: HTMLElement,
  leftReport: ReportJson,
  rightReport: ReportJson,
  leftLabel: string,
  rightLabel: string
): void {
  const compareWebsites = buildCompareWebsites(leftReport, rightReport);

  const headerHtml = [
    '<section class="compare-sort-row">',
    '<div class="compare-column">',
    '<h3 class="compare-column-header">' + renderCompareHeaderHtml(leftReport.metadata, leftLabel) + '</h3>',
    renderCompareSortControls("left"),
    '</div>',
    '<div class="compare-column">',
    '<h3 class="compare-column-header">' + renderCompareHeaderHtml(rightReport.metadata, rightLabel) + '</h3>',
    renderCompareSortControls("right"),
    '</div>',
    '</section>'
  ].join("");

  compareResults.innerHTML = headerHtml + compareWebsites.map((website) => {
    return [
      '<section class="compare-row" data-website-name="' + escapeHtml(website.name) + '">',
      '<div class="compare-column">',
      renderCompareCell(website.left, "Not present in left report."),
      '</div>',
      '<div class="compare-column">',
      renderCompareCell(website.right, "Not present in right report."),
      '</div>',
      '</section>'
    ].join("");
  }).join("");
  compareResults.hidden = false;
}

function getCompareSortValue(row: HTMLElement, side: CompareSide, datasetKey: SortDatasetKey): bigint {
  const selector = side === "left"
    ? ".compare-column:first-of-type details.site"
    : ".compare-column:last-of-type details.site";
  const site = row.querySelector<HTMLDetailsElement>(selector);
  if (!site) {
    return 0n;
  }
  return BigInt(site.dataset[datasetKey] ?? "0");
}

function setActiveCompareSortButton(activeBtn: HTMLButtonElement, compareResults: HTMLElement): void {
  const buttons = compareResults.querySelectorAll<HTMLButtonElement>(".compare-sort-controls button");
  for (const button of buttons) {
    button.classList.toggle("active", button === activeBtn);
  }
}

function sortCompareRows(
  compareResults: HTMLElement,
  side: CompareSide,
  datasetKey: SortDatasetKey,
  activeBtn: HTMLButtonElement
): void {
  const rows = Array.from(compareResults.querySelectorAll<HTMLElement>(":scope > .compare-row"));
  rows.sort((a, b) => {
    const av = getCompareSortValue(a, side, datasetKey);
    const bv = getCompareSortValue(b, side, datasetKey);
    if (av !== bv) {
      return av > bv ? -1 : 1;
    }
    return (a.dataset.websiteName ?? "").localeCompare(b.dataset.websiteName ?? "");
  });
  for (const row of rows) {
    compareResults.appendChild(row);
  }
  setActiveCompareSortButton(activeBtn, compareResults);
}

function installCompareSortHandlers(compareResults: HTMLElement): void {
  const sortButtons = compareResults.querySelectorAll<HTMLButtonElement>(".compare-sort-controls button");
  for (const button of sortButtons) {
    button.addEventListener("click", () => {
      const side = button.dataset.compareSide;
      const sortKey = button.dataset.sortKey;
      if ((side !== "left" && side !== "right") || (sortKey !== "totalCycles" && sortKey !== "slowRejectCycles")) {
        return;
      }
      sortCompareRows(compareResults, side, sortKey, button);
    });
  }
}

function syncPairedDetails(leftDetails: HTMLDetailsElement, rightDetails: HTMLDetailsElement): void {
  let syncing = false;
  const installSync = (source: HTMLDetailsElement, target: HTMLDetailsElement): void => {
    source.addEventListener("toggle", () => {
      if (syncing) {
        return;
      }
      syncing = true;
      target.open = source.open;
      syncing = false;
    });
  };

  installSync(leftDetails, rightDetails);
  installSync(rightDetails, leftDetails);
}

function syncCompareDetails(compareResults: HTMLElement): void {
  const compareRows = Array.from(compareResults.querySelectorAll<HTMLElement>(":scope > .compare-row"));
  for (const row of compareRows) {
    const leftWebsiteDetails = row.querySelector<HTMLDetailsElement>(".compare-column:first-of-type details.site");
    const rightWebsiteDetails = row.querySelector<HTMLDetailsElement>(".compare-column:last-of-type details.site");
    if (!leftWebsiteDetails || !rightWebsiteDetails) {
      continue;
    }

    syncPairedDetails(leftWebsiteDetails, rightWebsiteDetails);

    const leftBreakdowns = Array.from(
      leftWebsiteDetails.querySelectorAll<HTMLDetailsElement>("details.selector-breakdown")
    );
    const rightBreakdowns = Array.from(
      rightWebsiteDetails.querySelectorAll<HTMLDetailsElement>("details.selector-breakdown")
    );
    const pairCount = Math.min(leftBreakdowns.length, rightBreakdowns.length);
    for (let index = 0; index < pairCount; index += 1) {
      const leftBreakdown = leftBreakdowns[index];
      const rightBreakdown = rightBreakdowns[index];
      if (leftBreakdown && rightBreakdown) {
        syncPairedDetails(leftBreakdown, rightBreakdown);
      }
    }
  }
}

function setActive(activeBtn: HTMLButtonElement, byTotal: HTMLButtonElement, bySlow: HTMLButtonElement): void {
  byTotal.classList.toggle("active", activeBtn === byTotal);
  bySlow.classList.toggle("active", activeBtn === bySlow);
}

function sortBy(
  datasetKey: SortDatasetKey,
  activeBtn: HTMLButtonElement,
  list: HTMLElement,
  byTotal: HTMLButtonElement,
  bySlow: HTMLButtonElement
): void {
  const siteElements = Array.from(list.querySelectorAll<HTMLDetailsElement>(":scope > details.site"));
  siteElements.sort((a, b) => {
    const av = BigInt(a.dataset[datasetKey] ?? "0");
    const bv = BigInt(b.dataset[datasetKey] ?? "0");
    if (av === bv) {
      return 0;
    }
    return av > bv ? -1 : 1;
  });
  for (const site of siteElements) {
    list.appendChild(site);
  }
  setActive(activeBtn, byTotal, bySlow);
}

function isReportMetadataJson(value: unknown): value is ReportMetadataJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return (record.branch === null || typeof record.branch === "string")
    && (record.commit_hash === null || typeof record.commit_hash === "string")
    && (record.dirty === null || typeof record.dirty === "boolean")
    && (record.tagline === null || typeof record.tagline === "string")
    && (record.time_end === undefined || record.time_end === null || typeof record.time_end === "string")
    && (record.report_source === undefined || isReportSource(record.report_source));
}

function isCountingStatsJson(value: unknown): value is CountingStatsJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.sharing_instances)
    && isFiniteNumber(record.selector_map_hits)
    && isFiniteNumber(record.fast_rejects)
    && isFiniteNumber(record.slow_rejects)
    && isFiniteNumber(record.slow_accepts);
}

function isTimingsJsonBody(value: unknown): value is TimingsJsonBody {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.updating_bloom_filter_cycles)
    && isFiniteNumber(record.slow_rejecting_cycles)
    && isFiniteNumber(record.slow_accepting_cycles)
    && isFiniteNumber(record.fast_rejecting_cycles)
    && isFiniteNumber(record.checking_style_sharing_cycles)
    && isFiniteNumber(record.inserting_into_sharing_cache_cycles)
    && isFiniteNumber(record.querying_selector_map_cycles);
}

function isTimingStatsJson(value: unknown): value is TimingStatsJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isTimingsJsonBody(record.means) && isTimingsJsonBody(record.stddevs);
}

function isBenchmarkRunSummaryJson(value: unknown): value is BenchmarkRunSummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.mean_cycles)
    && isCountingStatsJson(record.counts)
    && isTimingStatsJson(record.times);
}

function isSelectorStatsJson(value: unknown): value is SelectorStatsJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  const means = getRecord(record.means_cycles);
  const stddevs = getRecord(record.stddevs_cycles);
  if (means === null || stddevs === null) {
    return false;
  }
  return Object.values(means).every(isFiniteNumber)
    && Object.values(stddevs).every(isFiniteNumber);
}

function isSelectorsSummaryJson(value: unknown): value is SelectorsSummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isSelectorStatsJson(record.baseline)
    && isSelectorStatsJson(record.fail_caches)
    && isSelectorStatsJson(record.after_preprocessing);
}

function isFailCachePreprocessingSummaryJson(value: unknown): value is FailCachePreprocessingSummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isFiniteNumber(record.mean_interning_cycles);
}

function isSummaryJson(value: unknown): value is SummaryJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return isBenchmarkRunSummaryJson(record.baseline)
    && isBenchmarkRunSummaryJson(record.fail_caches)
    && isFailCachePreprocessingSummaryJson(record.fail_cache_preprocessing)
    && isPreprocessingSummaryJson(record.preprocessing)
    && isBenchmarkRunSummaryJson(record.after_preprocessing);
}

function isWebsiteJson(value: unknown): value is WebsiteJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  return typeof record.website === "string"
    && isSummaryJson(record.summary)
    && isSelectorsSummaryJson(record.selector_slow_rejects_summary);
}

function isReportJson(value: unknown): value is ReportJson {
  const record = getRecord(value);
  if (record === null) {
    return false;
  }
  if (!Array.isArray(record.websites)) {
    return false;
  }
  return isReportMetadataJson(record.metadata)
    && record.websites.every((website) => isWebsiteJson(website));
}

async function main(): Promise<void> {
  const list = document.getElementById("websites-list");
  const byTotal = document.getElementById("sort-total");
  const bySlow = document.getElementById("sort-slow");
  const status = document.getElementById("report-status");
  const commitLine = document.getElementById("report-commit-line");
  const compareControls = document.getElementById("compare-controls");
  const compareLeftSelectField = document.getElementById("compare-left-select-field");
  const compareRightSelectField = document.getElementById("compare-right-select-field");
  const compareLeftFileField = document.getElementById("compare-left-file-field");
  const compareRightFileField = document.getElementById("compare-right-file-field");
  const compareLeft = document.getElementById("compare-left");
  const compareRight = document.getElementById("compare-right");
  const compareLeftFile = document.getElementById("compare-left-file");
  const compareRightFile = document.getElementById("compare-right-file");
  const compareStatus = document.getElementById("compare-status");
  const compareRun = document.getElementById("compare-run");
  const compareLeftOnly = document.getElementById("compare-left-only");
  const compareRightOnly = document.getElementById("compare-right-only");
  const compareResults = document.getElementById("compare-results");
  const sortControls = document.querySelector(".sort-controls");
  if (!(list instanceof HTMLElement)
    || !(byTotal instanceof HTMLButtonElement)
    || !(bySlow instanceof HTMLButtonElement)
    || !(status instanceof HTMLElement)
    || !(commitLine instanceof HTMLElement)
    || !(compareControls instanceof HTMLElement)
    || !(compareLeftSelectField instanceof HTMLElement)
    || !(compareRightSelectField instanceof HTMLElement)
    || !(compareLeftFileField instanceof HTMLElement)
    || !(compareRightFileField instanceof HTMLElement)
    || !(compareLeft instanceof HTMLSelectElement)
    || !(compareRight instanceof HTMLSelectElement)
    || !(compareLeftFile instanceof HTMLInputElement)
    || !(compareRightFile instanceof HTMLInputElement)
    || !(compareStatus instanceof HTMLElement)
    || !(compareRun instanceof HTMLButtonElement)
    || !(compareLeftOnly instanceof HTMLButtonElement)
    || !(compareRightOnly instanceof HTMLButtonElement)
    || !(compareResults instanceof HTMLElement)
    || !(sortControls instanceof HTMLElement)) {
    return;
  }

  byTotal.addEventListener("click", () => {
    sortBy("totalCycles", byTotal, list, byTotal, bySlow);
  });
  bySlow.addEventListener("click", () => {
    sortBy("slowRejectCycles", bySlow, list, byTotal, bySlow);
  });

  try {
    const response = await fetch("report.json");
    if (!response.ok) {
      throw new Error("HTTP " + response.status + " while loading report.json");
    }

    const raw: unknown = await response.json();
    if (!isReportJson(raw)) {
      throw new Error("report.json had an unexpected shape");
    }

    renderMetadata(raw.metadata, commitLine);
    renderDefaultReportList(list, byTotal, bySlow, raw);
    list.hidden = false;
    hideCompareResults(compareResults);
    document.body.classList.remove("compare-active");
    status.hidden = true;
    await loadCompareControls(
      compareControls,
      compareLeftSelectField,
      compareRightSelectField,
      compareLeftFileField,
      compareRightFileField,
      compareLeft,
      compareRight,
      compareLeftFile,
      compareRightFile,
      compareStatus,
      raw.metadata
    );
    installCompareHandler(
      raw,
      compareRun,
      compareLeftOnly,
      compareRightOnly,
      getReportSource(raw.metadata),
      compareLeft,
      compareRight,
      compareLeftFile,
      compareRightFile,
      compareStatus,
      list,
      sortControls,
      compareResults,
      byTotal,
      bySlow
    );
  } catch (error: unknown) {
    status.hidden = false;
    status.classList.add("error");
    const message = error instanceof Error ? error.message : String(error);
    status.textContent = "Failed to load report data: " + message;
  }
}

void main();
