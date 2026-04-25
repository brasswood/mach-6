type SegmentKind =
  | "indexing"
  | "otherPreprocessing"
  | "updatingBloomFilter"
  | "checkingStyleSharing"
  | "queryingSelectorMap"
  | "fastRejecting"
  | "slowRejecting"
  | "slowAccepting"
  | "insertingIntoSharingCache"
  | "other";

type SortDatasetKey = "totalNs" | "slowRejectNs";

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
  tagline: string | null;
  time_end?: string | null;
}

interface WebsiteJson {
  website: string;
  summary: SummaryJson;
  selector_slow_rejects_summary: SelectorsSummaryJson;
}

interface SummaryJson {
  before_preprocessing: BenchmarkRunSummaryJson;
  preprocessing: PreprocessingSummaryJson;
  after_preprocessing: BenchmarkRunSummaryJson;
}

interface PreprocessingSummaryJson {
  mean_indexing_duration_ns: number;
  mean_overall_duration_ns: number;
}

interface BenchmarkRunSummaryJson {
  mean_duration_ns: number;
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
  updating_bloom_filter_ns: number;
  slow_rejecting_ns: number;
  slow_accepting_ns: number;
  fast_rejecting_ns: number;
  checking_style_sharing_ns: number;
  inserting_into_sharing_cache_ns: number;
  querying_selector_map_ns: number;
}

interface SelectorsSummaryJson {
  before_preprocessing: SelectorStatsJson;
  after_preprocessing: SelectorStatsJson;
}

interface SelectorStatsJson {
  means_ns: Record<string, number>;
  stddevs_ns: Record<string, number>;
}

interface SegmentInfo {
  label: string;
  cssClass: string;
}

interface SelectorRow {
  selector: string;
  meanNs: bigint;
  stddevNs: bigint;
}

interface SegmentView {
  kind: SegmentKind;
  meanNs: bigint;
  stddevNs: bigint | null;
}

interface BarView {
  label: string;
  segments: SegmentView[];
  totalDurationNs: bigint;
  totalLengthNs: bigint;
  slowRejectNs: bigint;
  counts: CountingStatsJson;
  topSlowRejectSelectors: SelectorRow[];
}

interface WebsiteView {
  name: string;
  bars: BarView[];
  totalSortKeyNs: bigint;
  slowRejectSortKeyNs: bigint;
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
  "indexing",
  "otherPreprocessing",
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
  indexing: { label: "Indexing", cssClass: "seg-index" },
  otherPreprocessing: { label: "Other Preprocessing", cssClass: "seg-preprocess-other" },
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

function formatDuration(ns: bigint): string {
  const value = Number(ns);
  if (value >= 1_000_000) {
    return (value / 1_000_000).toFixed(3) + " ms";
  }
  if (value >= 1_000) {
    return (value / 1_000).toFixed(3) + " us";
  }
  return value.toString() + " ns";
}

function formatSignedDuration(ns: bigint): string {
  return ns < 0n ? "-" + formatDuration(-ns) : formatDuration(ns);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function meanWithStddev(meanNs: bigint, stddevNs: bigint): string {
  return formatDuration(meanNs) + " \u00B1 " + formatDuration(stddevNs);
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

async function loadCompareControls(
  container: HTMLElement,
  leftSelect: HTMLSelectElement,
  rightSelect: HTMLSelectElement,
  compareStatus: HTMLElement,
  currentMetadata: ReportMetadataJson
): Promise<void> {
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

function setCompareStatus(compareStatus: HTMLElement, message: string, isError: boolean): void {
  compareStatus.hidden = false;
  compareStatus.classList.toggle("error", isError);
  compareStatus.textContent = message;
}

function installCompareHandler(
  compareButton: HTMLButtonElement,
  leftSelect: HTMLSelectElement,
  rightSelect: HTMLSelectElement,
  compareStatus: HTMLElement,
  list: HTMLElement,
  sortControls: HTMLElement,
  compareResults: HTMLElement
): void {
  compareButton.addEventListener("click", async () => {
    compareButton.disabled = true;
    setCompareStatus(compareStatus, "Loading selected reports...", false);

    try {
      const leftUrl = leftSelect.value;
      const rightUrl = rightSelect.value;
      const [leftReport, rightReport] = await Promise.all([
        fetchReportJson(leftUrl),
        fetchReportJson(rightUrl)
      ]);

      const leftLabel = leftSelect.selectedOptions[0]?.textContent ?? "Left report";
      const rightLabel = rightSelect.selectedOptions[0]?.textContent ?? "Right report";
      renderCompareResults(compareResults, leftReport, rightReport, leftLabel, rightLabel);
      list.hidden = true;
      sortControls.hidden = true;
      document.body.classList.add("compare-active");
      setCompareStatus(
        compareStatus,
        "Showing compare view for " + leftReport.websites.length + " left websites and "
          + rightReport.websites.length + " right websites.",
        false
      );
    } catch (error: unknown) {
      const message = error instanceof Error ? error.message : String(error);
      compareResults.hidden = true;
      document.body.classList.remove("compare-active");
      setCompareStatus(compareStatus, "Failed to load compare reports: " + message, true);
    } finally {
      compareButton.disabled = false;
    }
  });
}

function buildSelectorRows(stats: SelectorStatsJson): SelectorRow[] {
  const rows = Object.entries(stats.means_ns).map(([selector, meanNs]) => {
    const stddevNs = stats.stddevs_ns[selector];
    if (stddevNs === undefined) {
      throw new Error("Missing stddev for selector: " + selector);
    }
    return {
      selector,
      meanNs: toBigInt(meanNs),
      stddevNs: toBigInt(stddevNs)
    };
  });
  rows.sort((left, right) => {
    if (left.meanNs === right.meanNs) {
      return left.selector.localeCompare(right.selector);
    }
    return left.meanNs > right.meanNs ? -1 : 1;
  });
  return rows.slice(0, MAX_SLOW_REJECT_ROWS);
}

function buildBar(
  label: string,
  summary: BenchmarkRunSummaryJson,
  selectorsSummary: SelectorStatsJson,
  includePreprocessing: PreprocessingSummaryJson | null
): BarView {
  const means = summary.times.means;
  const stddevs = summary.times.stddevs;
  const measuredMatchDurations: SegmentView[] = [
    { kind: "updatingBloomFilter", meanNs: toBigInt(means.updating_bloom_filter_ns), stddevNs: toBigInt(stddevs.updating_bloom_filter_ns) },
    { kind: "checkingStyleSharing", meanNs: toBigInt(means.checking_style_sharing_ns), stddevNs: toBigInt(stddevs.checking_style_sharing_ns) },
    { kind: "queryingSelectorMap", meanNs: toBigInt(means.querying_selector_map_ns), stddevNs: toBigInt(stddevs.querying_selector_map_ns) },
    { kind: "fastRejecting", meanNs: toBigInt(means.fast_rejecting_ns), stddevNs: toBigInt(stddevs.fast_rejecting_ns) },
    { kind: "slowRejecting", meanNs: toBigInt(means.slow_rejecting_ns), stddevNs: toBigInt(stddevs.slow_rejecting_ns) },
    { kind: "slowAccepting", meanNs: toBigInt(means.slow_accepting_ns), stddevNs: toBigInt(stddevs.slow_accepting_ns) },
    { kind: "insertingIntoSharingCache", meanNs: toBigInt(means.inserting_into_sharing_cache_ns), stddevNs: toBigInt(stddevs.inserting_into_sharing_cache_ns) }
  ];
  const measuredMatchSum = measuredMatchDurations.reduce((sum, segment) => {
    return sum + segment.meanNs;
  }, 0n);
  measuredMatchDurations.push({
    kind: "other",
    meanNs: toBigInt(summary.mean_duration_ns) - measuredMatchSum,
    stddevNs: null
  });

  const segments: SegmentView[] = [];
  if (includePreprocessing) {
    const indexingNs = toBigInt(includePreprocessing.mean_indexing_duration_ns);
    const overallNs = toBigInt(includePreprocessing.mean_overall_duration_ns);
    segments.push({ kind: "indexing", meanNs: indexingNs, stddevNs: null });
    segments.push({ kind: "otherPreprocessing", meanNs: overallNs - indexingNs, stddevNs: null });
  }
  segments.push(...measuredMatchDurations);

  const totalDurationNs = segments.reduce((sum, segment) => {
    return sum + segment.meanNs;
  }, 0n);
  const totalLengthNs = segments.reduce((sum, segment) => {
    return sum + (segment.meanNs > 0n ? segment.meanNs : 0n);
  }, 0n);
  const slowRejectSegment = segments.find((segment) => segment.kind === "slowRejecting");
  if (!slowRejectSegment) {
    throw new Error("Missing slow-reject segment for " + label);
  }

  return {
    label,
    segments,
    totalDurationNs,
    totalLengthNs,
    slowRejectNs: slowRejectSegment.meanNs,
    counts: summary.counts,
    topSlowRejectSelectors: buildSelectorRows(selectorsSummary)
  };
}

function buildWebsiteView(website: WebsiteJson): WebsiteView {
  const bars = [
    buildBar("Before Preprocessing", website.summary.before_preprocessing, website.selector_slow_rejects_summary.before_preprocessing, null),
    buildBar("With Preprocessing", website.summary.after_preprocessing, website.selector_slow_rejects_summary.after_preprocessing, website.summary.preprocessing)
  ];

  const totalSortKeyNs = bars.reduce((max, bar) => {
    return bar.totalDurationNs > max ? bar.totalDurationNs : max;
  }, 0n);
  const slowRejectSortKeyNs = bars.reduce((max, bar) => {
    return bar.slowRejectNs > max ? bar.slowRejectNs : max;
  }, 0n);

  const legendKinds = SEGMENT_ORDER.filter((kind) => {
    return bars.some((bar) => {
      return bar.segments.some((segment) => segment.kind === kind);
    });
  });

  return {
    name: website.website,
    bars,
    totalSortKeyNs,
    slowRejectSortKeyNs,
    legendKinds: [...legendKinds]
  };
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

function renderSummaryBar(bar: BarView, pageMaxBarLengthNs: bigint): string {
  const segmentsHtml = bar.segments.map((segment) => {
    return '<div class="bar-seg ' + SEGMENT_INFO[segment.kind].cssClass + '" style="width: ' + pct(segment.meanNs > 0n ? segment.meanNs : 0n, bar.totalLengthNs) + '%"></div>';
  }).join("");
  const warningClass = bar.totalLengthNs !== bar.totalDurationNs ? " warning" : "";
  const displayNote = bar.totalLengthNs !== bar.totalDurationNs
    ? '<div class="time-display-note">Displayed: ' + escapeHtml(formatDuration(bar.totalLengthNs)) + '</div>'
    : "";

  return [
    '<div class="variant-summary">',
    '<div class="variant-label">' + escapeHtml(bar.label) + '</div>',
    '<div class="bar-wrap"><div class="bar-total" style="width: ' + pct(bar.totalLengthNs, pageMaxBarLengthNs) + '%">' + segmentsHtml + '</div></div>',
    '<div class="time"><div class="time-value' + warningClass + '">' + escapeHtml(formatDuration(bar.totalDurationNs)) + '</div>' + displayNote + '</div>',
    '</div>'
  ].join("");
}

function renderExpandedBar(bar: BarView): string {
  const segmentsHtml = bar.segments.map((segment) => {
    return '<div class="expanded-bar-seg ' + SEGMENT_INFO[segment.kind].cssClass + '" style="width: ' + pct(segment.meanNs > 0n ? segment.meanNs : 0n, bar.totalLengthNs) + '%"></div>';
  }).join("");
  const legendHtml = bar.segments.map((segment) => {
    const warningClass = segment.meanNs < 0n ? "legend-warning" : "";
    const value = segment.stddevNs === null
      ? formatSignedDuration(segment.meanNs)
      : formatSignedDuration(segment.meanNs) + " \u00B1 " + formatDuration(segment.stddevNs);
    return '<span class="' + warningClass + '">' + renderSegmentSwatch(segment.kind) + ': ' + escapeHtml(value) + '</span>';
  }).join("") + '<span>Total: ' + escapeHtml(formatDuration(bar.totalDurationNs)) + '</span>';

  return [
    '<section class="expanded-chart">',
    '<h5>Timing Breakdown</h5>',
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
      '<td class="col-time"><div class="cell-scroll">' + escapeHtml(meanWithStddev(row.meanNs, row.stddevNs)) + '</div></td>',
      '</tr>'
    ].join("");
  }).join("");
}

function renderSelectorBreakdownContent(bar: BarView): string {
  return [
    '<div class="selector-breakdown-inner">',
    '<table class="selector-breakdown-table">',
    '<thead><tr><th class="col-selector">Selector</th><th class="col-time">Total Slow Reject Time</th></tr></thead>',
    '<tbody>' + renderSelectorRows(bar.topSlowRejectSelectors) + '</tbody>',
    '</table>',
    '</div>'
  ].join("");
}

function renderSelectorBreakdown(bar: BarView): string {
  return [
    '<details class="selector-breakdown">',
    '<summary>Slow-Reject Timings Aggregated by Selector (Top ' + MAX_SLOW_REJECT_ROWS + ')</summary>',
    renderSelectorBreakdownContent(bar),
    '</details>'
  ].join("");
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
    renderSelectorBreakdown(bar),
    '</section>'
  ].join("");
}

function renderVariantDetailsWithoutBreakdown(bar: BarView): string {
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
    '</section>'
  ].join("");
}

function renderWebsiteSummaryContent(website: WebsiteView, pageMaxBarLengthNs: bigint): string {
  return [
    '<div class="row">',
    '<div class="chevron" aria-hidden="true"></div>',
    '<div class="name">' + escapeHtml(website.name) + '</div>',
    '<div class="summary-variants">' + website.bars.map((bar) => {
      return renderSummaryBar(bar, pageMaxBarLengthNs);
    }).join("") + '</div>',
    '</div>',
    '<div class="bar-legend">' + website.legendKinds.map((kind) => {
      return '<span>' + renderSegmentSwatch(kind) + '</span>';
    }).join("") + '</div>'
  ].join("");
}

function renderWebsiteDetailsContent(website: WebsiteView): string {
  return '<div class="details-variants">' + website.bars.map(renderVariantDetails).join("") + '</div>';
}

function renderWebsite(website: WebsiteView, pageMaxBarLengthNs: bigint): string {
  return [
    '<details class="site" data-total-ns="' + website.totalSortKeyNs.toString() + '" data-slow-reject-ns="' + website.slowRejectSortKeyNs.toString() + '">',
    '<summary>',
    renderWebsiteSummaryContent(website, pageMaxBarLengthNs),
    '</summary>',
    '<div class="details">',
    renderWebsiteDetailsContent(website),
    '</div>',
    '</details>'
  ].join("");
}

function buildWebsiteMap(websites: WebsiteJson[]): Map<string, WebsiteView> {
  const websiteMap = new Map<string, WebsiteView>();
  for (const website of websites) {
    websiteMap.set(website.website, buildWebsiteView(website));
  }
  return websiteMap;
}

function getPageMaxBarLengthNs(websites: WebsiteView[]): bigint {
  return websites.reduce((max, website) => {
    return website.bars.reduce((innerMax, bar) => {
      return bar.totalLengthNs > innerMax ? bar.totalLengthNs : innerMax;
    }, max);
  }, 0n);
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
  pageMaxBarLengthNs: bigint,
  missingLabel: string
): string {
  if (website === null) {
    return '<p class="compare-empty">' + escapeHtml(missingLabel) + '</p>';
  }
  return renderWebsiteSummaryContent(website, pageMaxBarLengthNs);
}

function renderCompareSelectorBreakdownCard(
  bar: BarView | null,
  missingLabel: string
): string {
  if (bar === null) {
    return '<p class="compare-empty">' + escapeHtml(missingLabel) + '</p>';
  }
  return renderSelectorBreakdownContent(bar);
}

function renderCompareVariantDetailsCard(
  bar: BarView | null,
  missingLabel: string
): string {
  if (bar === null) {
    return '<p class="compare-empty">' + escapeHtml(missingLabel) + '</p>';
  }
  return renderVariantDetailsWithoutBreakdown(bar);
}

function renderCompareVariantPair(
  leftBar: BarView | null,
  rightBar: BarView | null
): string {
  const title = leftBar?.label ?? rightBar?.label ?? "Variant";
  return [
    '<details class="selector-breakdown compare-selector-breakdown">',
    '<summary>Slow-Reject Timings Aggregated by Selector for ' + escapeHtml(title) + ' (Top ' + MAX_SLOW_REJECT_ROWS + ')</summary>',
    '<div class="compare-row compare-variant-row">',
      '<div class="compare-column">',
      renderCompareVariantDetailsCard(leftBar, "Not present in left report."),
      '</div>',
      '<div class="compare-column">',
      renderCompareVariantDetailsCard(rightBar, "Not present in right report."),
      '</div>',
      '</div>',
    '<div class="compare-row selector-breakdown-inner compare-breakdown-row">',
      '<div class="compare-column">',
      renderCompareSelectorBreakdownCard(leftBar, "Not present in left report."),
      '</div>',
      '<div class="compare-column">',
      renderCompareSelectorBreakdownCard(rightBar, "Not present in right report."),
    '</div>',
    '</div>',
    '</details>'
  ].join("");
}

function renderCompareWebsiteBreakdowns(
  leftWebsite: WebsiteView | null,
  rightWebsite: WebsiteView | null
): string {
  const leftBars = leftWebsite?.bars ?? [];
  const rightBars = rightWebsite?.bars ?? [];
  const maxBars = Math.max(leftBars.length, rightBars.length);
  const sections: string[] = [];

  for (let index = 0; index < maxBars; index += 1) {
    const leftBar = leftBars[index] ?? null;
    const rightBar = rightBars[index] ?? null;
    sections.push(
      renderCompareVariantPair(leftBar, rightBar)
    );
  }

  return sections.join("");
}

function renderCompareCard(
  metadata: ReportMetadataJson,
  fallbackLabel: string,
  website: WebsiteView | null,
  pageMaxBarLengthNs: bigint,
  missingLabel: string
): string {
  return [
    '<div class="compare-card">',
    '<h3 class="compare-column-header">' + renderCompareHeaderHtml(metadata, fallbackLabel) + '</h3>',
    renderCompareCell(website, pageMaxBarLengthNs, missingLabel),
    '</div>'
  ].join("");
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

function renderCompareResults(
  compareResults: HTMLElement,
  leftReport: ReportJson,
  rightReport: ReportJson,
  leftLabel: string,
  rightLabel: string
): void {
  const compareWebsites = buildCompareWebsites(leftReport, rightReport);
  const leftWebsites = compareWebsites.flatMap((website) => {
    return website.left === null ? [] : [website.left];
  });
  const rightWebsites = compareWebsites.flatMap((website) => {
    return website.right === null ? [] : [website.right];
  });
  const leftPageMaxBarLengthNs = getPageMaxBarLengthNs(leftWebsites);
  const rightPageMaxBarLengthNs = getPageMaxBarLengthNs(rightWebsites);

  compareResults.innerHTML = compareWebsites.map((website) => {
    return [
      '<details class="site compare-site">',
      '<summary>',
      '<div class="compare-row">',
      '<div class="compare-column">',
      renderCompareCard(
        leftReport.metadata,
        leftLabel,
        website.left,
        leftPageMaxBarLengthNs,
        "Not present in left report."
      ),
      '</div>',
      '<div class="compare-column">',
      renderCompareCard(
        rightReport.metadata,
        rightLabel,
        website.right,
        rightPageMaxBarLengthNs,
        "Not present in right report."
      ),
      '</div>',
      '</div>',
      '</summary>',
      '<div class="details">',
      renderCompareWebsiteBreakdowns(website.left, website.right),
      '</div>',
      '</details>'
    ].join("");
  }).join("");
  compareResults.hidden = false;
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

function isReportJson(value: unknown): value is ReportJson {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Array.isArray(record.websites) && typeof record.metadata === "object" && record.metadata !== null;
}

async function main(): Promise<void> {
  const list = document.getElementById("websites-list");
  const byTotal = document.getElementById("sort-total");
  const bySlow = document.getElementById("sort-slow");
  const status = document.getElementById("report-status");
  const commitLine = document.getElementById("report-commit-line");
  const compareControls = document.getElementById("compare-controls");
  const compareLeft = document.getElementById("compare-left");
  const compareRight = document.getElementById("compare-right");
  const compareStatus = document.getElementById("compare-status");
  const compareRun = document.getElementById("compare-run");
  const compareResults = document.getElementById("compare-results");
  const sortControls = document.querySelector(".sort-controls");
  if (!(list instanceof HTMLElement)
    || !(byTotal instanceof HTMLButtonElement)
    || !(bySlow instanceof HTMLButtonElement)
    || !(status instanceof HTMLElement)
    || !(commitLine instanceof HTMLElement)
    || !(compareControls instanceof HTMLElement)
    || !(compareLeft instanceof HTMLSelectElement)
    || !(compareRight instanceof HTMLSelectElement)
    || !(compareStatus instanceof HTMLElement)
    || !(compareRun instanceof HTMLButtonElement)
    || !(compareResults instanceof HTMLElement)
    || !(sortControls instanceof HTMLElement)) {
    return;
  }

  byTotal.addEventListener("click", () => {
    sortBy("totalNs", byTotal, list, byTotal, bySlow);
  });
  bySlow.addEventListener("click", () => {
    sortBy("slowRejectNs", bySlow, list, byTotal, bySlow);
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
    const websites = raw.websites.map(buildWebsiteView);
    const pageMaxBarLengthNs = websites.reduce((max, website) => {
      return website.bars.reduce((innerMax, bar) => {
        return bar.totalLengthNs > innerMax ? bar.totalLengthNs : innerMax;
      }, max);
    }, 0n);

    list.innerHTML = websites.map((website) => {
      return renderWebsite(website, pageMaxBarLengthNs);
    }).join("");
    list.hidden = false;
    document.body.classList.remove("compare-active");
    status.hidden = true;
    sortBy("totalNs", byTotal, list, byTotal, bySlow);
    await loadCompareControls(compareControls, compareLeft, compareRight, compareStatus, raw.metadata);
    installCompareHandler(compareRun, compareLeft, compareRight, compareStatus, list, sortControls, compareResults);
  } catch (error: unknown) {
    status.hidden = false;
    status.classList.add("error");
    const message = error instanceof Error ? error.message : String(error);
    status.textContent = "Failed to load report data: " + message;
  }
}

void main();
