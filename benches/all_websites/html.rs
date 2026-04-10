enum ReportBar<'a> {
    BeforePreprocessing {
        label: &'static str,
        matching: &'a MatchBenchResult,
    },
    WithPreprocessing {
        label: &'static str,
        matching: &'a MatchBenchResult,
        preprocessing: &'a PreprocessingResult,
    },
}

struct WebsiteReportView<'a> {
    website: &'a str,
    json_file: String,
    bars: [ReportBar<'a>; 2],
    summary_total_ns: u128,
    summary_slow_reject_ns: u128,
}

fn render_index_html(results: &[WebsiteResult]) -> String {
    let report_views: Vec<_> = results.iter().map(WebsiteReportView::from_result).collect();

    let max_duration_ns = report_views
        .iter()
        .flat_map(|view| view.bars.iter())
        .map(|bar| bar.total_duration().as_nanos())
        .max()
        .unwrap_or(1)
        .max(1);

    let mut sections = String::new();
    for view in &report_views {
        let website = escape_html(view.website);
        let before_summary = render_summary_variant(&view.bars[0], max_duration_ns);
        let after_summary = render_summary_variant(&view.bars[1], max_duration_ns);
        let before_details = render_detail_variant(&view.bars[0]);
        let after_details = render_detail_variant(&view.bars[1]);
        sections.push_str(&format!(
            r#"
<details class="site" data-total-ns="{total_ns}" data-slow-reject-ns="{slow_reject_ns}">
  <summary>
    <div class="row">
      <div class="chevron" aria-hidden="true"></div>
      <div class="name">{website}</div>
      <div class="summary-variants">
        {before_summary}
        {after_summary}
      </div>
    </div>
    <div class="bar-legend">
      {compact_legend}
    </div>
  </summary>
  <div class="details">
    <div class="details-variants">
      {before_details}
      {after_details}
    </div>
    <p><a href="{json_file}">JSON data</a></p>
  </div>
</details>
"#,
            website = website,
            total_ns = view.summary_total_ns,
            slow_reject_ns = view.summary_slow_reject_ns,
            before_summary = before_summary,
            after_summary = after_summary,
            compact_legend = render_compact_legend(),
            before_details = before_details,
            after_details = after_details,
            json_file = escape_html(&view.json_file),
        ));
    }

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Mach 6 Benchmark Report</title>
  <style>
    :root {{
      --bg: #f7f7f3;
      --fg: #1d232b;
      --muted: #5b6470;
      --bar: #0f766e;
      --bar-bg: #d9e5e3;
      --card: #ffffff;
      --line: #d8d8d0;
    }}
    body {{
      margin: 0;
      padding: 24px;
      font-family: "Segoe UI", Tahoma, Geneva, Verdana, sans-serif;
      color: var(--fg);
      background: linear-gradient(180deg, #f7f7f3 0%, #f2f5f8 100%);
    }}
    main {{
      max-width: 980px;
      margin: 0 auto;
    }}
    h1 {{
      margin: 0 0 4px 0;
      font-size: 26px;
    }}
    .subtitle {{
      margin: 0 0 20px 0;
      color: var(--muted);
    }}
    .sort-controls {{
      display: flex;
      gap: 8px;
      margin: 0 0 14px 0;
      flex-wrap: wrap;
    }}
    .sort-btn {{
      border: 1px solid var(--line);
      background: #fff;
      color: var(--fg);
      padding: 6px 10px;
      border-radius: 6px;
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
    }}
    .sort-btn.active {{
      background: #e7f2ef;
      border-color: #8fb8af;
    }}
    .site {{
      background: var(--card);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 10px 12px;
      margin-bottom: 10px;
    }}
    summary {{
      list-style: none;
      cursor: pointer;
    }}
    summary::-webkit-details-marker {{
      display: none;
    }}
    .row {{
      display: grid;
      grid-template-columns: 12px minmax(150px, 220px) minmax(260px, 1fr);
      align-items: start;
      gap: 12px;
    }}
    .chevron {{
      width: 0;
      height: 0;
      border-top: 5px solid transparent;
      border-bottom: 5px solid transparent;
      border-left: 7px solid var(--muted);
      transform: rotate(0deg);
      transform-origin: 40% 50%;
      transition: transform 120ms ease-out;
    }}
    details[open] .chevron {{
      transform: rotate(90deg);
    }}
    .name {{
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
      font-weight: 600;
    }}
    .summary-variants {{
      display: grid;
      gap: 8px;
    }}
    .variant-summary {{
      display: grid;
      grid-template-columns: minmax(130px, 170px) minmax(120px, 1fr) 120px;
      align-items: center;
      gap: 10px;
    }}
    .variant-label {{
      color: var(--muted);
      font-size: 12px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.02em;
    }}
    .bar-wrap {{
      width: 100%;
      height: 28px;
      background: var(--bar-bg);
      overflow: hidden;
    }}
    .bar-total {{
      display: flex;
      height: 100%;
      min-width: 0;
    }}
    .bar-seg {{
      height: 100%;
      min-width: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      overflow: hidden;
      white-space: nowrap;
      font-size: 11px;
      color: #0f172a;
      font-weight: 600;
    }}
    .seg-slow {{
      background: #f59e0b;
    }}
    .seg-bloom {{
      background: #06b6d4;
    }}
    .seg-fast {{
      background: #ef4444;
    }}
    .seg-slow-accept {{
      background: #14b8a6;
    }}
    .seg-share-check {{
      background: #3b82f6;
    }}
    .seg-share-insert {{
      background: #8b5cf6;
    }}
    .seg-query {{
      background: #22c55e;
    }}
    .seg-other {{
      background: var(--bar);
    }}
    .seg-index {{
      background: #c2410c;
    }}
    .seg-preprocess-other {{
      background: #fb7185;
    }}
    .bar-legend {{
      margin-top: 8px;
      margin-left: 24px;
      display: flex;
      flex-wrap: wrap;
      gap: 8px 14px;
      color: var(--muted);
      font-size: 12px;
    }}
    .expanded-chart {{
      margin-bottom: 14px;
    }}
    .expanded-chart h5 {{
      margin: 0 0 8px 0;
      color: var(--muted);
      font-size: 13px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.02em;
    }}
    .expanded-bar-wrap {{
      width: 100%;
      height: 32px;
      background: var(--bar-bg);
      overflow: hidden;
    }}
    .expanded-bar-total {{
      display: flex;
      width: 100%;
      height: 100%;
      min-width: 0;
    }}
    .expanded-bar-seg {{
      height: 100%;
      min-width: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      overflow: hidden;
      white-space: nowrap;
      font-size: 12px;
      color: #0f172a;
      font-weight: 700;
    }}
    .expanded-legend {{
      margin-top: 7px;
      display: flex;
      flex-wrap: wrap;
      gap: 10px 18px;
      color: var(--muted);
      font-size: 12px;
    }}
    .swatch {{
      display: inline-block;
      width: 10px;
      height: 10px;
      margin-right: 6px;
      vertical-align: -1px;
    }}
    .time {{
      text-align: right;
      font-variant-numeric: tabular-nums;
    }}
    .details {{
      padding-top: 12px;
      margin-top: 10px;
      border-top: 1px solid var(--line);
    }}
    .details-variants {{
      display: grid;
      gap: 18px;
    }}
    .variant-details {{
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 12px;
      background: #fcfdfb;
    }}
    .variant-details-title {{
      margin: 0 0 12px 0;
      color: var(--fg);
      font-size: 14px;
      font-weight: 700;
    }}
    .selector-breakdown {{
      margin-top: 12px;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 8px 10px;
      background: #fafcfb;
    }}
    .selector-breakdown > summary {{
      display: flex;
      align-items: center;
      gap: 8px;
      font-weight: 600;
    }}
    .selector-breakdown > summary::before {{
      content: "";
      width: 0;
      height: 0;
      border-top: 5px solid transparent;
      border-bottom: 5px solid transparent;
      border-left: 7px solid var(--muted);
      transform: rotate(0deg);
      transform-origin: 40% 50%;
      transition: transform 120ms ease-out;
      flex: 0 0 auto;
    }}
    .selector-breakdown[open] > summary::before {{
      transform: rotate(90deg);
    }}
    .selector-breakdown-inner {{
      margin-top: 8px;
    }}
    .selector-view-controls {{
      display: flex;
      gap: 8px;
      margin-top: 8px;
      margin-bottom: 8px;
      flex-wrap: wrap;
    }}
    .selector-view-btn {{
      border: 1px solid var(--line);
      background: #fff;
      color: var(--fg);
      padding: 4px 8px;
      border-radius: 6px;
      font-size: 12px;
      font-weight: 600;
      cursor: pointer;
    }}
    .selector-view-btn.active {{
      background: #e7f2ef;
      border-color: #8fb8af;
    }}
    .selector-view.hidden {{
      display: none;
    }}
    .selector-breakdown-table {{
      table-layout: fixed;
      width: 100%;
      max-width: 100%;
    }}
    .selector-breakdown-table code {{
      white-space: nowrap;
    }}
    .selector-breakdown-table th,
    .selector-breakdown-table td {{
      width: auto;
      max-width: none;
    }}
    .selector-breakdown-table .col-element {{
      width: 38%;
    }}
    .selector-breakdown-table .col-selector {{
      width: 38%;
    }}
    .selector-breakdown-table .col-source {{
      width: 14%;
    }}
    .selector-breakdown-table .col-time {{
      width: 10%;
      white-space: nowrap;
    }}
    .selector-breakdown-table-selectors .col-selector {{
      width: 80%;
    }}
    .selector-breakdown-table-selectors .col-time {{
      width: 20%;
    }}
    .cell-scroll {{
      overflow-x: auto;
      overflow-y: hidden;
      white-space: nowrap;
      max-width: 100%;
    }}
    .muted-inline {{
      color: var(--muted);
      font-size: 12px;
    }}
    table {{
      border-collapse: collapse;
      width: 100%;
      max-width: 600px;
    }}
    th, td {{
      text-align: left;
      padding: 4px 8px 4px 0;
      vertical-align: top;
    }}
    th {{
      width: 260px;
      color: var(--muted);
      font-weight: 600;
    }}
    p {{
      margin: 10px 0 0 0;
    }}
    @media (max-width: 700px) {{
      .row {{
        grid-template-columns: 12px 1fr;
        gap: 6px;
      }}
      .summary-variants {{
        grid-column: 2 / 3;
      }}
      .bar-legend {{
        margin-left: 18px;
      }}
      .variant-summary {{
        grid-template-columns: 1fr;
        gap: 6px;
      }}
      .expanded-bar-wrap {{
        height: 28px;
      }}
      .time {{
        text-align: left;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <h1>Mach 6 Benchmark Report</h1>
    <p class="subtitle">Each row shows total runtime; expand for detailed selector statistics and raw JSON.</p>
    <div class="sort-controls" role="group" aria-label="Sort websites">
      <button id="sort-total" class="sort-btn" type="button">Sort by Overall Time</button>
      <button id="sort-slow" class="sort-btn" type="button">Sort by Slow-Reject Time</button>
    </div>
    <section id="websites-list">
      {sections}
    </section>
  </main>
  <script>
    (function () {{
      const list = document.getElementById("websites-list");
      const byTotal = document.getElementById("sort-total");
      const bySlow = document.getElementById("sort-slow");
      if (!list || !byTotal || !bySlow) return;

      function setActive(activeBtn) {{
        byTotal.classList.toggle("active", activeBtn === byTotal);
        bySlow.classList.toggle("active", activeBtn === bySlow);
      }}

      function sortBy(datasetKey, activeBtn) {{
        const sites = Array.from(list.querySelectorAll(":scope > details.site"));
        sites.sort((a, b) => {{
          const av = BigInt(a.dataset[datasetKey] || "0");
          const bv = BigInt(b.dataset[datasetKey] || "0");
          if (av === bv) return 0;
          return av > bv ? -1 : 1;
        }});
        for (const site of sites) {{
          list.appendChild(site);
        }}
        setActive(activeBtn);
      }}

      byTotal.addEventListener("click", function () {{
        sortBy("totalNs", byTotal);
      }});
      bySlow.addEventListener("click", function () {{
        sortBy("slowRejectNs", bySlow);
      }});

      document.addEventListener("click", function (event) {{
        const target = event.target;
        if (!(target instanceof HTMLElement)) return;
        const button = target.closest(".selector-view-btn");
        if (!button) return;
        const breakdown = button.closest(".selector-breakdown");
        if (!breakdown) return;

        const view = button.dataset.view;
        const pairs = breakdown.querySelector(".selector-view-pairs");
        const selectors = breakdown.querySelector(".selector-view-selectors");
        if (!pairs || !selectors) return;

        const buttons = breakdown.querySelectorAll(".selector-view-btn");
        for (const b of buttons) {{
          b.classList.toggle("active", b === button);
        }}
        const showSelectors = view === "selectors";
        pairs.classList.toggle("hidden", showSelectors);
        selectors.classList.toggle("hidden", !showSelectors);
      }});

      sortBy("totalNs", byTotal);
    }})();
  </script>
</body>
</html>
"#,
        sections = sections
    )
}

impl<'a> WebsiteReportView<'a> {
    fn from_result(result: &'a WebsiteResult) -> Self {
        let bars = 
        [
            ReportBar::BeforePreprocessing {
                label: "Before preprocessing",
                matching: &result.before_preprocessing,
            },
            ReportBar::WithPreprocessing {
                label: "With preprocessing",
                matching: &result.after_preprocessing,
                preprocessing: &result.preprocessing,
            },
        ];
        // assert that the sum of component times (without other) does not exceed the overall duration measured
        for bar in &bars {
            let measured_sum = bar.measured_sum();
            let total_duration = bar.total_duration();
            if measured_sum > total_duration {
                panic!(
                    "Measured timing sum exceeded total duration: measured_sum={}, total_duration={}, website={}, bar={}",
                    format_duration(measured_sum),
                    format_duration(total_duration),
                    result.website,
                    bar.label(),
                );
            }
        }
        let summary_total_ns = bars
            .iter()
            .map(|bar| bar.total_duration().as_nanos())
            .max()
            .unwrap_or(0);
        let summary_slow_reject_ns = result
            .before_preprocessing
            .stats
            .times
            .slow_rejecting
            .as_nanos()
            .max(result.after_preprocessing.stats.times.slow_rejecting.as_nanos());

        Self {
            website: &result.website,
            json_file: format!("json/{}.json", make_filename_safe(&result.website)),
            bars,
            summary_total_ns,
            summary_slow_reject_ns,
        }
    }
}

impl ReportBar<'_> {
    fn label(&self) -> &'static str {
        match self {
            ReportBar::BeforePreprocessing { label, .. } => label,
            ReportBar::WithPreprocessing { label, .. } => label,
        }
    }

    fn matching(&self) -> &MatchBenchResult {
        match self {
            ReportBar::BeforePreprocessing { matching, .. } => matching,
            ReportBar::WithPreprocessing { matching, .. } => matching,
        }
    }

    fn preprocessing(&self) -> Option<&PreprocessingResult> {
        match self {
            ReportBar::BeforePreprocessing { .. } => None,
            ReportBar::WithPreprocessing { preprocessing, .. } => Some(preprocessing),
        }
    }

    fn total_duration(&self) -> Duration {
        self.matching().duration
            + self
                .preprocessing()
                .map(|p| p.preprocessing_duration)
                .unwrap_or(Duration::ZERO)
    }

    fn preprocessing_breakdown(&self) -> (Duration, Duration) {
        self
            .preprocessing()
            .map(|p| {
                let other_duration = p.preprocessing_duration.saturating_sub(p.indexing_duration);
                (p.indexing_duration, other_duration)
            })
            .unwrap_or((Duration::ZERO, Duration::ZERO))
    }

    fn matching_breakdown(&self) -> TimingStats {
        self.matching().stats.times
    }

    fn measured_sum(&self) -> Duration {
        let (indexing_duration, preprocessing_other) = self.preprocessing_breakdown();
        let TimingStats {
            updating_bloom_filter,
            slow_rejecting,
            slow_accepting,
            fast_rejecting,
            checking_style_sharing,
            inserting_into_sharing_cache,
            querying_selector_map,
            ..
        } = self.matching_breakdown();
        updating_bloom_filter
            + indexing_duration
            + preprocessing_other
            + slow_rejecting
            + slow_accepting
            + fast_rejecting
            + checking_style_sharing
            + inserting_into_sharing_cache
            + querying_selector_map
    }

    fn other_duration(&self) -> Duration {
        self.total_duration().saturating_sub(self.measured_sum())
    }
}

fn render_summary_variant(bar: &ReportBar<'_>, max_duration_ns: u128) -> String {
    let total_duration = bar.total_duration();
    let total_width_pct = (total_duration.as_nanos() as f64 / max_duration_ns as f64) * 100.0;
    let (summary_bar_segments, _, _) = render_variant_chart_parts(bar);

    format!(
        r#"<div class="variant-summary">
  <div class="variant-label">{label}</div>
  <div class="bar-wrap">
    <div class="bar-total" style="width: {total_width_pct:.2}%">
      {summary_bar_segments}
    </div>
  </div>
  <div class="time">{total_time}</div>
</div>"#,
        label = escape_html(bar.label()),
        total_width_pct = total_width_pct,
        summary_bar_segments = summary_bar_segments,
        total_time = format_duration(total_duration),
    )
}

fn render_detail_variant(bar: &ReportBar<'_>) -> String {
    let result = bar.matching();
    let total_duration = bar.total_duration();
    let (_, expanded_bar_segments, expanded_legend) = render_variant_chart_parts(bar);
    let mut selector_rows_html = String::new();
    for row in &result.selector_slow_reject_rows {
        selector_rows_html.push_str(&format!(
            r#"<tr>
  <td class="col-element"><div class="cell-scroll"><code>{element_html}</code> <span class="muted-inline">(id: {element_id})</span></div></td>
  <td class="col-selector"><div class="cell-scroll"><code>{selector_css}</code></div></td>
  <td class="col-source"><div class="cell-scroll">{source}</div></td>
  <td class="col-time"><div class="cell-scroll">{slow_reject_time}</div></td>
</tr>"#,
            element_html = escape_html(&row.element_html),
            element_id = row.element_id,
            selector_css = escape_html(&row.selector_css),
            source = row.source,
            slow_reject_time = format_duration(row.slow_reject_time),
        ));
    }
    if selector_rows_html.is_empty() {
        selector_rows_html.push_str(r#"<tr><td colspan="4">No selector stats captured.</td></tr>"#);
    }

    let mut selector_totals_rows_html = String::new();
    for row in &result.selector_total_slow_reject_rows {
        selector_totals_rows_html.push_str(&format!(
            r#"<tr>
  <td class="col-selector"><div class="cell-scroll"><code>{selector_css}</code></div></td>
  <td class="col-time"><div class="cell-scroll">{total_slow_reject_time}</div></td>
</tr>"#,
            selector_css = escape_html(&row.selector_css),
            total_slow_reject_time = format_duration(row.total_slow_reject_time),
        ));
    }
    if selector_totals_rows_html.is_empty() {
        selector_totals_rows_html.push_str(r#"<tr><td colspan="2">No selector stats captured.</td></tr>"#);
    }

    format!(
        r#"<section class="variant-details">
  <h4 class="variant-details-title">{label}</h4>
  <section class="expanded-chart">
    <h5>Timing Breakdown</h5>
    <div class="expanded-bar-wrap">
      <div class="expanded-bar-total">
        {expanded_bar_segments}
      </div>
    </div>
    <div class="expanded-legend">
      {expanded_legend}
      <span>Total: {total_time}</span>
    </div>
  </section>
  <table>
    <tbody>
      <tr><th>Sharing Instances</th><td>{sharing_instances}</td></tr>
      <tr><th>Selector Map Hits</th><td>{selector_map_hits}</td></tr>
      <tr><th>Fast Rejects</th><td>{fast_rejects}</td></tr>
      <tr><th>Slow Rejects</th><td>{slow_rejects}</td></tr>
      <tr><th>Slow Accepts</th><td>{slow_accepts}</td></tr>
    </tbody>
  </table>
  <details class="selector-breakdown">
    <summary>Slow-Reject Timings Aggregated by Selector (Top {max_selector_rows})</summary>
    <div class="selector-view-controls" role="group" aria-label="Selector timing view">
      <button class="selector-view-btn active" type="button" data-view="selectors">Top Selectors</button>
      <button class="selector-view-btn" type="button" data-view="pairs">Top Pairs</button>
    </div>
    <div class="selector-breakdown-inner">
      <div class="selector-view selector-view-selectors">
        <table class="selector-breakdown-table selector-breakdown-table-selectors">
          <thead>
            <tr>
              <th>Selector</th>
              <th>Total Slow Reject Time</th>
            </tr>
          </thead>
          <tbody>
            {selector_totals_rows_html}
          </tbody>
        </table>
      </div>
      <div class="selector-view selector-view-pairs hidden">
        <table class="selector-breakdown-table">
          <thead>
            <tr>
              <th>Element</th>
              <th>Selector</th>
              <th>Source</th>
              <th>Slow Reject Time</th>
            </tr>
          </thead>
          <tbody>
            {selector_rows_html}
          </tbody>
        </table>
      </div>
    </div>
  </details>
</section>"#,
        label = escape_html(bar.label()),
        expanded_bar_segments = expanded_bar_segments,
        expanded_legend = expanded_legend,
        total_time = format_duration(total_duration),
        sharing_instances = format_usize(result.stats.sharing_instances),
        selector_map_hits = format_usize(result.stats.selector_map_hits),
        fast_rejects = format_usize(result.stats.fast_rejects),
        slow_rejects = format_usize(result.stats.slow_rejects),
        slow_accepts = format_usize(result.stats.slow_accepts),
        max_selector_rows = MAX_SELECTOR_ROWS_PER_WEBSITE,
        selector_rows_html = selector_rows_html,
        selector_totals_rows_html = selector_totals_rows_html,
    )
}

fn render_variant_chart_parts(
    bar: &ReportBar<'_>,
) -> (String, String, String) {
    let total_duration = bar.total_duration();
    let (indexing_duration, preprocessing_other) = bar.preprocessing_breakdown();
    let TimingStats {
        updating_bloom_filter,
        slow_rejecting,
        slow_accepting,
        fast_rejecting,
        checking_style_sharing,
        inserting_into_sharing_cache,
        querying_selector_map,
        ..
    } = bar.matching_breakdown();
    let other_duration = bar.other_duration();
    let pct = |duration: Duration| -> f64 {
        if total_duration.is_zero() {
            0.0
        } else {
            (duration.as_nanos() as f64 / total_duration.as_nanos() as f64) * 100.0
        }
    };

    let mut summary_bar_segments = String::new();
    let mut expanded_bar_segments = String::new();
    let mut segment_rows = Vec::new();
    if matches!(bar, ReportBar::WithPreprocessing { .. }) {
        segment_rows.push(("seg-index", pct(indexing_duration)));
        segment_rows.push(("seg-preprocess-other", pct(preprocessing_other)));
    }
    segment_rows.extend([
        ("seg-bloom", pct(updating_bloom_filter)),
        ("seg-share-check", pct(checking_style_sharing)),
        ("seg-query", pct(querying_selector_map)),
        ("seg-fast", pct(fast_rejecting)),
        ("seg-slow", pct(slow_rejecting)),
        ("seg-slow-accept", pct(slow_accepting)),
        ("seg-share-insert", pct(inserting_into_sharing_cache)),
        ("seg-other", pct(other_duration)),
    ]);
    for (class_name, segment_pct) in segment_rows {
        if segment_pct <= 0.0 {
            continue;
        }
        summary_bar_segments.push_str(&format!(
            r#"<div class="bar-seg {class_name}" style="width: {segment_pct:.2}%"></div>"#,
            class_name = class_name,
            segment_pct = segment_pct,
        ));
        expanded_bar_segments.push_str(&format!(
            r#"<div class="expanded-bar-seg {class_name}" style="width: {segment_pct:.2}%"></div>"#,
            class_name = class_name,
            segment_pct = segment_pct,
        ));
    }

    let legend_item = |class_name: &str, name: &str, duration: Duration| -> String {
        format!(
            r#"<span><i class="swatch {class_name}"></i>{name}: {duration}</span>"#,
            class_name = class_name,
            name = name,
            duration = format_duration(duration),
        )
    };
    let mut expanded_legend = String::new();
    let mut legend_rows = Vec::new();
    if matches!(bar, ReportBar::WithPreprocessing { .. }) {
        legend_rows.push(("seg-index", "Indexing", indexing_duration));
        legend_rows.push(("seg-preprocess-other", "Other Preprocessing", preprocessing_other));
    }
    legend_rows.extend([
        ("seg-bloom", "Updating Bloom Filter", updating_bloom_filter),
        ("seg-share-check", "Checking Style Sharing", checking_style_sharing),
        ("seg-query", "Querying Selector Map", querying_selector_map),
        ("seg-fast", "Fast Rejecting", fast_rejecting),
        ("seg-slow", "Slow Rejecting", slow_rejecting),
        ("seg-slow-accept", "Slow Accepting", slow_accepting),
        ("seg-share-insert", "Inserting Into Sharing Cache", inserting_into_sharing_cache),
        ("seg-other", "Other", other_duration),
    ]);
    for (class_name, name, duration) in legend_rows {
        expanded_legend.push_str(&legend_item(class_name, name, duration));
    }

    (summary_bar_segments, expanded_bar_segments, expanded_legend)
}

fn render_compact_legend() -> String {
    let mut compact_legend = String::new();
    for (class_name, name) in [
        ("seg-index", "Indexing"),
        ("seg-preprocess-other", "Other Preprocessing"),
        ("seg-bloom", "Updating Bloom Filter"),
        ("seg-share-check", "Checking Style Sharing"),
        ("seg-query", "Querying Selector Map"),
        ("seg-fast", "Fast Rejecting"),
        ("seg-slow", "Slow Rejecting"),
        ("seg-slow-accept", "Slow Accepting"),
        ("seg-share-insert", "Inserting Into Sharing Cache"),
        ("seg-other", "Other"),
    ] {
        compact_legend.push_str(&format!(
            r#"<span><i class="swatch {class_name}"></i>{name}</span>"#,
            class_name = class_name,
            name = name,
        ));
    }
    compact_legend
}

fn format_usize(value: usize) -> String {
    value.to_formatted_string(&Locale::en)
}

fn make_filename_safe(string: &str) -> String {
    let mut string = string.replace(
        &['?', '"', '/', '\\', '*', '<', '>', ':', '|', '^'][..],
        "_",
    );
    if string.len() > 240 {
        let mut boundary = 240;
        while boundary > 0 && !string.is_char_boundary(boundary) {
            boundary -= 1;
        }
        string.truncate(boundary);
    }
    string
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
