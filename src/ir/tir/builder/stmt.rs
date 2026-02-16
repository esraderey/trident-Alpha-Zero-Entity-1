//! Block and statement compilation.

use std::collections::BTreeMap;

use crate::ast::*;
use crate::tir::TIROp;

use super::layout::resolve_type_width;
use super::TIRBuilder;

// ─── Block and statement emission ─────────────────────────────────

impl TIRBuilder {
    pub(crate) fn build_block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.build_stmt(&stmt.node);
        }
        if let Some(tail) = &block.tail_expr {
            self.build_expr(&tail.node);
        }
    }

    pub(crate) fn build_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Let {
                pattern, init, ty, ..
            } => {
                self.build_expr(&init.node);

                match pattern {
                    Pattern::Name(name) => {
                        if name.node != "_" {
                            if let Some(top) = self.stack.last_mut() {
                                top.name = Some(name.node.clone());
                            }
                            // If type is an array, record elem_width.
                            if let Some(sp_ty) = ty {
                                if let Type::Array(inner_ty, _) = &sp_ty.node {
                                    let ew = resolve_type_width(inner_ty, &self.target_config);
                                    if let Some(top) = self.stack.last_mut() {
                                        top.elem_width = Some(ew);
                                    }
                                }
                            }
                            // Record struct field layout from struct init.
                            if let Expr::StructInit { fields, .. } = &init.node {
                                let mut field_map = BTreeMap::new();
                                let widths = self.compute_struct_field_widths(ty, fields);
                                let total: u32 = widths.iter().sum();
                                let mut offset = 0u32;
                                for (i, (fname, _)) in fields.iter().enumerate() {
                                    let fw = widths.get(i).copied().unwrap_or(1);
                                    let from_top = total - offset - fw;
                                    field_map.insert(fname.node.clone(), (from_top, fw));
                                    offset += fw;
                                }
                                self.struct_layouts.insert(name.node.clone(), field_map);
                            } else if let Some(sp_ty) = ty {
                                self.register_struct_layout_from_type(&name.node, &sp_ty.node);
                            }
                        }
                    }
                    Pattern::Tuple(names) => {
                        let top = self.stack.pop();
                        if let Some(entry) = top {
                            let total_width = entry.width;
                            let n = names.len() as u32;
                            let elem_width = if n > 0 { total_width / n } else { 1 };

                            for name in names.iter() {
                                let var_name = if name.node == "_" {
                                    "__anon"
                                } else {
                                    &name.node
                                };
                                self.stack.push_named(var_name, elem_width);
                                self.flush_stack_effects();
                            }

                            // Eagerly pop trailing wildcard bindings.
                            // For `let (h1, _, _, _, _) = digest`, wildcards on top
                            // of the stack are immediately discarded.
                            let mut trailing_wildcards = 0u32;
                            for name in names.iter().rev() {
                                if name.node == "_" {
                                    trailing_wildcards += elem_width;
                                } else {
                                    break;
                                }
                            }
                            if trailing_wildcards > 0 {
                                for _ in 0..(trailing_wildcards / elem_width) {
                                    self.stack.pop();
                                }
                                self.emit_pop(trailing_wildcards);
                            }
                        }
                    }
                }
            }

            Stmt::Assign { place, value } => {
                if let Place::Var(name) = &place.node {
                    self.build_expr(&value.node);
                    let depth = self.stack.access_var(name);
                    self.flush_stack_effects();
                    if depth <= 15 {
                        self.ops.push(TIROp::Swap(depth));
                        self.ops.push(TIROp::Pop(1));
                    }
                    self.stack.pop();
                }
            }

            Stmt::If {
                cond,
                then_block,
                else_block,
            } => {
                self.build_expr(&cond.node);
                self.stack.pop(); // cond consumed

                if let Some(else_blk) = else_block {
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved.clone());
                    let else_body = self.build_block_as_ir(&else_blk.node);
                    self.stack.restore_state(saved);

                    self.ops.push(TIROp::IfElse {
                        then_body,
                        else_body,
                    });
                } else {
                    let saved = self.stack.save_state();
                    let then_body = self.build_block_as_ir(&then_block.node);
                    self.stack.restore_state(saved);

                    self.ops.push(TIROp::IfOnly { then_body });
                }
            }

            Stmt::For {
                var: _,
                start: _,
                end,
                body,
                ..
            } => {
                let loop_label = self.fresh_label("loop");

                self.build_expr(&end.node);

                self.ops.push(TIROp::Call(loop_label.clone()));
                self.ops.push(TIROp::Pop(1));
                self.stack.pop();

                let saved = self.stack.save_state();
                self.stack.clear();
                let body_ir = self.build_block_as_ir(&body.node);
                self.stack.restore_state(saved);

                self.ops.push(TIROp::Loop {
                    label: loop_label,
                    body: body_ir,
                });
            }

            Stmt::TupleAssign { names, value } => {
                self.build_expr(&value.node);
                let top = self.stack.pop();
                if let Some(entry) = top {
                    let total_width = entry.width;
                    let n = names.len() as u32;
                    let elem_width = if n > 0 { total_width / n } else { 1 };

                    for name in names.iter().rev() {
                        let depth = self.stack.access_var(&name.node);
                        self.flush_stack_effects();
                        if elem_width == 1 {
                            self.ops.push(TIROp::Swap(depth));
                            self.ops.push(TIROp::Pop(1));
                        }
                    }
                    let _ = total_width;
                }
            }

            Stmt::Expr(expr) => {
                let before = self.stack.stack_len();
                self.build_expr(&expr.node);
                while self.stack.stack_len() > before {
                    if let Some(top) = self.stack.last() {
                        let w = top.width;
                        if w > 0 {
                            self.emit_pop(w);
                        }
                    }
                    self.stack.pop();
                }
            }

            Stmt::Return(value) => {
                if let Some(val) = value {
                    self.build_expr(&val.node);
                }
            }

            Stmt::Reveal { event_name, fields } => {
                let tag = match self.event_tags.get(&event_name.node).copied() {
                    Some(t) => t,
                    None => {
                        self.ops.push(TIROp::Comment(format!(
                            "BUG: unregistered event '{}'",
                            event_name.node
                        )));
                        0
                    }
                };
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();

                for def_name in &decl_order {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop();
                    }
                }

                self.ops.push(TIROp::Reveal {
                    name: event_name.node.clone(),
                    tag,
                    field_count: decl_order.len() as u32,
                });
            }

            Stmt::Asm {
                body,
                effect,
                target,
            } => {
                if let Some(tag) = target {
                    if tag != &self.target_config.name {
                        return;
                    }
                }

                self.stack.spill_all_named();
                self.flush_stack_effects();

                let lines: Vec<String> = body
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect();

                if !lines.is_empty() {
                    self.ops.push(TIROp::Asm {
                        lines,
                        effect: *effect,
                    });
                }

                if *effect > 0 {
                    for _ in 0..*effect {
                        self.stack.push_temp(1);
                    }
                } else if *effect < 0 {
                    for _ in 0..effect.unsigned_abs() {
                        self.stack.pop();
                    }
                }
            }

            Stmt::Match { expr, arms } => {
                self.build_match(expr, arms);
            }

            Stmt::Seal { event_name, fields } => {
                let tag = match self.event_tags.get(&event_name.node).copied() {
                    Some(t) => t,
                    None => {
                        self.ops.push(TIROp::Comment(format!(
                            "BUG: unregistered event '{}'",
                            event_name.node
                        )));
                        0
                    }
                };
                let decl_order = self
                    .event_defs
                    .get(&event_name.node)
                    .cloned()
                    .unwrap_or_default();
                let field_count = decl_order.len() as u32;

                // Push fields in reverse declaration order (so first declared
                // field ends up on top after all pushes).
                for def_name in decl_order.iter().rev() {
                    if let Some((_name, val)) = fields.iter().find(|(n, _)| n.node == *def_name) {
                        self.build_expr(&val.node);
                        self.stack.pop();
                    }
                }

                self.ops.push(TIROp::Seal {
                    name: event_name.node.clone(),
                    tag,
                    field_count,
                });
            }
        }
    }
}
