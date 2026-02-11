use std::path::Path;

use super::analyzer::{find_matching_brace, next_power_of_two, FunctionCost, ProgramCost};
use super::model::TableCost;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

// --- Report formatting ---

impl ProgramCost {
    /// Format a table-style cost report.
    pub fn format_report(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("Cost report: {}\n", self.program_name));
        out.push_str(&format!(
            "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
            "Function", "cc", "hash", "u32", "opst", "ram", "jump", "dominant"
        ));
        out.push_str(&"-".repeat(84));
        out.push('\n');

        for func in &self.functions {
            out.push_str(&format!(
                "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
                func.name,
                func.cost.processor,
                func.cost.hash,
                func.cost.u32_table,
                func.cost.op_stack,
                func.cost.ram,
                func.cost.jump_stack,
                func.cost.dominant_table(),
            ));
            if let Some((per_iter, bound)) = &func.per_iteration {
                out.push_str(&format!(
                    "  per iteration (x{})   {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}\n",
                    bound,
                    per_iter.processor,
                    per_iter.hash,
                    per_iter.u32_table,
                    per_iter.op_stack,
                    per_iter.ram,
                    per_iter.jump_stack,
                ));
            }
        }

        out.push_str(&"-".repeat(84));
        out.push('\n');
        out.push_str(&format!(
            "{:<24} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}  {}\n",
            "TOTAL",
            self.total.processor,
            self.total.hash,
            self.total.u32_table,
            self.total.op_stack,
            self.total.ram,
            self.total.jump_stack,
            self.total.dominant_table(),
        ));
        out.push('\n');
        out.push_str(&format!(
            "Padded height:           {}\n",
            self.padded_height
        ));
        out.push_str(&format!(
            "Program attestation:     {} hash rows\n",
            self.attestation_hash_rows
        ));
        out.push_str(&format!(
            "Estimated proving time:  ~{:.1}s\n",
            self.estimated_proving_secs
        ));

        // Power-of-2 boundary warning.
        let headroom = self.padded_height - self.total.max_height();
        if headroom < self.padded_height / 8 {
            out.push_str(&format!(
                "\nwarning: {} rows below padded height boundary ({})\n",
                headroom, self.padded_height
            ));
            out.push_str(&format!(
                "  adding {}+ rows to any table will double proving cost to {}\n",
                headroom + 1,
                self.padded_height * 2
            ));
        }

        out
    }

    /// Format a hotspots report (top N cost contributors).
    pub fn format_hotspots(&self, top_n: usize) -> String {
        let mut out = String::new();
        out.push_str(&format!("Top {} cost contributors:\n", top_n));

        let dominant = self.total.dominant_table();
        let dominant_total = match dominant {
            "hash" => self.total.hash,
            "u32" => self.total.u32_table,
            "ram" => self.total.ram,
            "proc" => self.total.processor,
            "opstack" => self.total.op_stack,
            _ => self.total.jump_stack,
        };

        let mut ranked: Vec<&FunctionCost> = self.functions.iter().collect();
        ranked.sort_by(|a, b| {
            let av = match dominant {
                "hash" => a.cost.hash,
                "u32" => a.cost.u32_table,
                "ram" => a.cost.ram,
                _ => a.cost.processor,
            };
            let bv = match dominant {
                "hash" => b.cost.hash,
                "u32" => b.cost.u32_table,
                "ram" => b.cost.ram,
                _ => b.cost.processor,
            };
            bv.cmp(&av)
        });

        for (i, func) in ranked.iter().take(top_n).enumerate() {
            let val = match dominant {
                "hash" => func.cost.hash,
                "u32" => func.cost.u32_table,
                "ram" => func.cost.ram,
                _ => func.cost.processor,
            };
            let pct = if dominant_total > 0 {
                (val as f64 / dominant_total as f64) * 100.0
            } else {
                0.0
            };
            out.push_str(&format!(
                "  {}. {:<24} {:>6} {} rows ({:.0}% of {} table)\n",
                i + 1,
                func.name,
                val,
                dominant,
                pct,
                dominant
            ));
        }

        out.push_str(&format!(
            "\nDominant table: {} ({} rows). Reduce {} operations to lower padded height.\n",
            dominant, dominant_total, dominant
        ));

        out
    }

    /// Generate optimization hints (H0001, H0002, H0004).
    /// H0001: hash table dominance — hash table is >2x taller than processor.
    /// H0002: headroom hint — significant room below next power-of-2 boundary.
    /// H0004: loop bound waste — declared bound >> constant iteration count.
    pub fn optimization_hints(&self) -> Vec<Diagnostic> {
        let mut hints = Vec::new();

        // H0001: Hash table dominance
        if self.total.hash > 0 && self.total.processor > 0 {
            let ratio = self.total.hash as f64 / self.total.processor as f64;
            if ratio > 2.0 {
                let mut diag = Diagnostic::warning(
                    format!(
                        "hint[H0001]: hash table is {:.1}x taller than processor table",
                        ratio
                    ),
                    Span::dummy(),
                );
                diag.notes
                    .push("processor optimizations will not reduce proving cost".to_string());
                diag.help = Some(
                    "consider: batching data before hashing, reducing Merkle depth, \
                     or using sponge_absorb_mem instead of repeated sponge_absorb"
                        .to_string(),
                );
                hints.push(diag);
            }
        }

        // H0002: Headroom hint (far below boundary = room to grow)
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;
        if headroom > self.padded_height / 4 && self.padded_height >= 16 {
            let headroom_pct = (headroom as f64 / self.padded_height as f64) * 100.0;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0002]: padded height is {}, but max table height is only {}",
                    self.padded_height, max_height
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "you have {} rows of headroom ({:.0}%) before the next doubling",
                headroom, headroom_pct
            ));
            diag.help = Some(format!(
                "this program could be {:.0}% more complex at zero additional proving cost",
                headroom_pct
            ));
            hints.push(diag);
        }

        // H0004: Loop bound waste
        for (fn_name, end_val, bound) in &self.loop_bound_waste {
            let ratio = *bound as f64 / *end_val as f64;
            let mut diag = Diagnostic::warning(
                format!(
                    "hint[H0004]: loop in '{}' bounded {} but iterates only {} times",
                    fn_name, bound, end_val
                ),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "declared bound is {:.0}x the actual iteration count",
                ratio
            ));
            diag.help = Some(format!(
                "tightening the bound to {} would reduce worst-case cost",
                next_power_of_two(*end_val)
            ));
            hints.push(diag);
        }

        hints
    }

    /// Serialize ProgramCost to a JSON string.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n  \"functions\": {\n");
        for (i, func) in self.functions.iter().enumerate() {
            out.push_str(&format!(
                "    \"{}\": {}",
                func.name,
                func.cost.to_json_value()
            ));
            if i + 1 < self.functions.len() {
                out.push(',');
            }
            out.push('\n');
        }
        out.push_str("  },\n");
        out.push_str(&format!("  \"total\": {},\n", self.total.to_json_value()));
        out.push_str(&format!("  \"padded_height\": {}\n", self.padded_height));
        out.push_str("}\n");
        out
    }

    /// Save cost analysis to a JSON file.
    pub fn save_json(&self, path: &Path) -> Result<(), String> {
        std::fs::write(path, self.to_json())
            .map_err(|e| format!("cannot write '{}': {}", path.display(), e))
    }

    /// Load cost analysis from a JSON file.
    pub fn load_json(path: &Path) -> Result<ProgramCost, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read '{}': {}", path.display(), e))?;
        Self::from_json(&content)
    }

    /// Parse a ProgramCost from a JSON string.
    pub fn from_json(s: &str) -> Result<ProgramCost, String> {
        // Extract "functions" block
        let fns_start = s
            .find("\"functions\"")
            .ok_or_else(|| "missing 'functions' key".to_string())?;
        let fns_obj_start = s[fns_start..]
            .find('{')
            .map(|i| fns_start + i)
            .ok_or_else(|| "missing functions object".to_string())?;

        // Find matching closing brace for functions object
        let fns_obj_end = find_matching_brace(s, fns_obj_start)
            .ok_or_else(|| "unmatched brace in functions".to_string())?;
        let fns_content = &s[fns_obj_start + 1..fns_obj_end];

        // Parse individual function entries
        let mut functions = Vec::new();
        let mut pos = 0;
        while pos < fns_content.len() {
            // Find next function name
            if let Some(quote_start) = fns_content[pos..].find('"') {
                let name_start = pos + quote_start + 1;
                if let Some(quote_end) = fns_content[name_start..].find('"') {
                    let name = fns_content[name_start..name_start + quote_end].to_string();
                    // Find the cost object for this function
                    let after_name = name_start + quote_end + 1;
                    if let Some(obj_start) = fns_content[after_name..].find('{') {
                        let abs_obj_start = after_name + obj_start;
                        if let Some(obj_end) = find_matching_brace(fns_content, abs_obj_start) {
                            let cost_str = &fns_content[abs_obj_start..=obj_end];
                            if let Some(cost) = TableCost::from_json_value(cost_str) {
                                functions.push(FunctionCost {
                                    name,
                                    cost,
                                    per_iteration: None,
                                });
                            }
                            pos = obj_end + 1;
                            continue;
                        }
                    }
                }
            }
            break;
        }

        // Extract "total"
        let total = {
            let total_start = s
                .find("\"total\"")
                .ok_or_else(|| "missing 'total' key".to_string())?;
            let obj_start = s[total_start..]
                .find('{')
                .map(|i| total_start + i)
                .ok_or_else(|| "missing total object".to_string())?;
            let obj_end = find_matching_brace(s, obj_start)
                .ok_or_else(|| "unmatched brace in total".to_string())?;
            TableCost::from_json_value(&s[obj_start..=obj_end])
                .ok_or_else(|| "invalid total cost".to_string())?
        };

        // Extract "padded_height"
        let padded_height = {
            let ph_start = s
                .find("\"padded_height\"")
                .ok_or_else(|| "missing 'padded_height' key".to_string())?;
            let rest = &s[ph_start + "\"padded_height\"".len()..];
            let colon = rest
                .find(':')
                .ok_or_else(|| "missing colon after padded_height".to_string())?;
            let after_colon = rest[colon + 1..].trim_start();
            let end = after_colon
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after_colon.len());
            after_colon[..end]
                .parse::<u64>()
                .map_err(|e| format!("invalid padded_height: {}", e))?
        };

        Ok(ProgramCost {
            program_name: String::new(),
            functions,
            total,
            attestation_hash_rows: 0,
            padded_height,
            estimated_proving_secs: 0.0,
            loop_bound_waste: Vec::new(),
        })
    }

    /// Format a comparison between this cost and another (old vs new).
    pub fn format_comparison(&self, other: &ProgramCost) -> String {
        let mut out = String::new();
        out.push_str("Cost comparison:\n");
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Function", "cc (old)", "cc (new)", "delta"
        ));
        out.push_str(&"-".repeat(48));
        out.push('\n');

        // Collect all function names from both
        let mut all_names: Vec<String> = Vec::new();
        for f in &self.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }
        for f in &other.functions {
            if !all_names.contains(&f.name) {
                all_names.push(f.name.clone());
            }
        }

        for name in &all_names {
            let old_cc = self
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.processor)
                .unwrap_or(0);
            let new_cc = other
                .functions
                .iter()
                .find(|f| f.name == *name)
                .map(|f| f.cost.processor)
                .unwrap_or(0);
            let delta = new_cc as i64 - old_cc as i64;
            let delta_str = if delta > 0 {
                format!("+{}", delta)
            } else if delta == 0 {
                "0".to_string()
            } else {
                format!("{}", delta)
            };
            out.push_str(&format!(
                "{:<20} {:>9} {:>9}  {:>6}\n",
                name, old_cc, new_cc, delta_str
            ));
        }

        out.push_str(&"-".repeat(48));
        out.push('\n');

        let old_total = self.total.processor;
        let new_total = other.total.processor;
        let total_delta = new_total as i64 - old_total as i64;
        let total_delta_str = if total_delta > 0 {
            format!("+{}", total_delta)
        } else if total_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", total_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "TOTAL", old_total, new_total, total_delta_str
        ));

        let old_ph = self.padded_height;
        let new_ph = other.padded_height;
        let ph_delta = new_ph as i64 - old_ph as i64;
        let ph_delta_str = if ph_delta > 0 {
            format!("+{}", ph_delta)
        } else if ph_delta == 0 {
            "0".to_string()
        } else {
            format!("{}", ph_delta)
        };
        out.push_str(&format!(
            "{:<20} {:>9} {:>9}  {:>6}\n",
            "Padded height:", old_ph, new_ph, ph_delta_str
        ));

        out
    }

    /// Generate diagnostics for power-of-2 boundary proximity.
    /// Warns when the program is within 12.5% of the next power-of-2 boundary.
    pub fn boundary_warnings(&self) -> Vec<Diagnostic> {
        let mut warnings = Vec::new();
        let max_height = self.total.max_height().max(self.attestation_hash_rows);
        let headroom = self.padded_height - max_height;

        if headroom < self.padded_height / 8 {
            let mut diag = Diagnostic::warning(
                format!("program is {} rows below padded height boundary", headroom),
                Span::dummy(),
            );
            diag.notes.push(format!(
                "padded_height = {} (max table height = {})",
                self.padded_height, max_height
            ));
            diag.notes.push(format!(
                "adding {}+ rows to any table will double proving cost to {}",
                headroom + 1,
                self.padded_height * 2
            ));
            diag.help = Some(format!(
                "consider optimizing to stay well below {}",
                self.padded_height
            ));
            warnings.push(diag);
        }

        warnings
    }
}
