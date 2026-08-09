//! Renders a `billing::Invoice` to a PDF file, via `printpdf`'s HTML/CSS
//! renderer ("html" feature). Gpui-free — depends only on `billing::Invoice`.
//!
//! Built-in PDF fonts don't cover Cyrillic, so a TTF with Cyrillic glyphs is
//! bundled and registered by name, referenced from the CSS via `font-family`.

use std::collections::BTreeMap;

use printpdf::{Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfSaveOptions};

use crate::billing::Invoice;

const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/PTSans-Regular.ttf");
const FONT_FAMILY: &str = "PT Sans";

/// Escape text for embedding in the invoice's HTML (project/client names and
/// task descriptions are free text and may contain `&`/`<`/`>`).
fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Build the invoice's HTML/CSS document (header with number/date + total,
/// a Проект/Заказчик/Ставка info row, the line-item table, a totals footer,
/// and the footnote) — mirrors the approved mockup's invoice layout.
pub fn build_invoice_html(invoice: &Invoice, project_name: &str, client: Option<&str>) -> String {
    let mut rows_html = String::new();
    for r in &invoice.rows {
        rows_html.push_str(&format!(
            "<tr><td class=\"nowrap\">{}</td><td>{}</td><td class=\"nowrap\">{}</td>\
             <td class=\"right nowrap\">{}</td><td class=\"right nowrap cell-amount\">{}</td></tr>",
            esc(&r.date_label),
            esc(&r.desc),
            esc(&r.range_label),
            esc(&r.hours_label),
            esc(&r.amount_label),
        ));
    }

    let table_or_empty = if invoice.rows.is_empty() {
        "<p class=\"empty\">Нет задач для выставления: укажите количество часов в панели выше \
         или запишите новое отработанное время.</p>"
            .to_string()
    } else {
        format!(
            "<table><thead><tr><th>Дата</th><th>Задача</th><th>Время выполнения</th>\
             <th class=\"right\">Часы</th><th class=\"right\">Сумма</th></tr></thead>\
             <tbody>{rows_html}</tbody></table>"
        )
    };

    let rest_sentence = invoice
        .rest_after_label
        .as_ref()
        .map(|r| format!(" Остаток к оплате после этого счёта: {}.", esc(r)))
        .unwrap_or_default();

    let client_label = client
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .unwrap_or("Без клиента");

    format!(
        r#"<html><head><style>
body {{ font-family: "{font}", sans-serif; color: #18181b; font-size: 10.5pt; }}
.head {{ display: flex; justify-content: space-between; align-items: flex-start;
  padding-bottom: 14px; border-bottom: 2px solid #18181b; }}
.title {{ font-size: 19pt; font-weight: bold; }}
.muted {{ color: #71717a; font-size: 9.5pt; margin-top: 4px; }}
.head-right {{ text-align: right; white-space: nowrap; }}
.label {{ text-transform: uppercase; font-size: 8pt; color: #a1a1aa; letter-spacing: 0.5px; }}
.amount-lg {{ font-size: 19pt; font-weight: bold; margin-top: 3px; white-space: nowrap; }}
.info-row {{ display: flex; margin-top: 18px; }}
.info-cell {{ flex: 1; }}
.value {{ font-size: 11pt; font-weight: bold; margin-top: 4px; }}
table {{ width: 100%; border-collapse: collapse; margin-top: 20px; font-size: 9.5pt; }}
th {{ text-align: left; font-size: 8pt; text-transform: uppercase; color: #71717a;
  border-bottom: 1px solid #d4d4d8; padding: 7px 8px 7px 0; }}
td {{ border-bottom: 1px solid #f1f1f3; padding: 8px 8px 8px 0; }}
.right {{ text-align: right; }}
.nowrap {{ white-space: nowrap; }}
.cell-amount {{ font-weight: 600; }}
.empty {{ color: #a1a1aa; font-size: 10pt; padding: 20px 0; }}
.totals {{ margin-top: 16px; margin-left: auto; width: 280px; }}
.totals-row {{ display: flex; justify-content: space-between; padding: 5px 0;
  font-size: 10pt; color: #52525b; white-space: nowrap; }}
.totals-row.grand {{ font-size: 13pt; font-weight: bold; color: #18181b; border-top: 1px solid #e4e4e8;
  margin-top: 4px; padding-top: 10px; }}
.footnote {{ margin-top: 22px; padding-top: 12px; border-top: 1px solid #ececef;
  font-size: 8.5pt; color: #a1a1aa; }}
</style></head>
<body>
  <div class="head">
    <div>
      <div class="title">Счёт на оплату</div>
      <div class="muted">{number} от {date}</div>
    </div>
    <div class="head-right">
      <div class="label">К оплате</div>
      <div class="amount-lg">{amount}</div>
    </div>
  </div>
  <div class="info-row">
    <div class="info-cell"><div class="label">Проект</div><div class="value">{project}</div></div>
    <div class="info-cell"><div class="label">Заказчик</div><div class="value">{client}</div></div>
    <div class="info-cell"><div class="label">Ставка</div><div class="value">{rate}</div></div>
  </div>
  {table_or_empty}
  <div class="totals">
    <div class="totals-row"><span>Всего часов</span><span>{total_hours}</span></div>
    <div class="totals-row"><span>Ставка</span><span>{rate}</span></div>
    <div class="totals-row grand"><span>Итого к&nbsp;оплате</span><span>{amount}</span></div>
  </div>
  <div class="footnote">В счёт включены задачи, по которым оплата ещё не поступала. Ранее оплачено по проекту: {paid_before}.{rest_sentence}</div>
</body></html>"#,
        font = FONT_FAMILY,
        number = esc(&invoice.number),
        date = esc(&invoice.date_label),
        amount = esc(&invoice.total_amount_label),
        project = esc(project_name),
        client = esc(client_label),
        rate = esc(&invoice.rate_label),
        table_or_empty = table_or_empty,
        total_hours = esc(&invoice.total_hours_label),
        paid_before = esc(&invoice.paid_before_label),
        rest_sentence = rest_sentence,
    )
}

/// Lay out `html` into a paginated A4 PDF and return its bytes. CPU-bound
/// (font shaping + layout) — safe on `cx.background_executor()`, touches no DB.
pub fn render_pdf(html: &str) -> anyhow::Result<Vec<u8>> {
    let mut fonts = BTreeMap::new();
    fonts.insert(FONT_FAMILY.to_string(), Base64OrRaw::Raw(FONT_BYTES.to_vec()));

    let options = GeneratePdfOptions {
        margin_top: Some(18.0),
        margin_right: Some(16.0),
        margin_bottom: Some(18.0),
        margin_left: Some(16.0),
        ..Default::default()
    };

    let mut warnings = Vec::new();
    let doc = PdfDocument::from_html(html, &BTreeMap::new(), &fonts, &options, &mut warnings)
        .map_err(|e| anyhow::anyhow!("render invoice PDF: {e}"))?;
    if doc.pages.is_empty() {
        let msgs: Vec<String> = warnings.iter().map(|w| w.msg.clone()).collect();
        anyhow::bail!("render invoice PDF: no pages produced ({})", msgs.join("; "));
    }

    let mut save_warnings = Vec::new();
    Ok(doc.save(&PdfSaveOptions::default(), &mut save_warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::billing::{build_invoice, BillableEntry};
    use crate::models::Currency;

    #[test]
    fn renders_a_real_pdf_with_cyrillic_text() {
        let entries = vec![BillableEntry {
            id: 1,
            minutes: 90,
            date_label: "24.06.2026".into(),
            range_label: "09:00 – 10:30".into(),
            desc: "Документация API".into(),
        }];
        let now = chrono::NaiveDate::from_ymd_opt(2026, 6, 24)
            .unwrap()
            .and_hms_opt(15, 0, 0)
            .unwrap();
        let invoice = build_invoice(&entries, 0, 90, 3500.0, Currency::Rub, 1, now);
        let html = build_invoice_html(&invoice, "Сайт Acme", Some("Acme Inc."));

        let bytes = render_pdf(&html).expect("pdf renders");
        assert!(bytes.starts_with(b"%PDF"), "not a PDF: {:?}", &bytes[..bytes.len().min(16)]);
        assert!(bytes.len() > 1000, "suspiciously small PDF: {} bytes", bytes.len());
    }

    #[test]
    fn empty_invoice_still_renders() {
        let invoice = build_invoice(&[], 0, 0, 3500.0, Currency::Rub, 1, chrono::Local::now().naive_local());
        let html = build_invoice_html(&invoice, "Project", None);
        let bytes = render_pdf(&html).expect("pdf renders even with no rows");
        assert!(bytes.starts_with(b"%PDF"));
    }
}
