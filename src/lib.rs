#![deny(clippy::all)]

use core::str;
use std::{collections::HashMap, sync::Arc};

use farmfe_core::{
  config::Config,
  context::CompilationContext,
  error::Result,
  plugin::{Plugin, PluginFinalizeResourcesHookParams},
  resource::ResourceType,
  swc_common::SourceMap,
  swc_ecma_ast::{
    ArrayLit, CallExpr, Callee, Expr, ExprOrSpread, Ident, KeyValueProp, Lit, MemberExpr,
    MemberProp, MetaPropKind, ObjectLit, Prop, PropName, PropOrSpread,
  },
};
use farmfe_toolkit::{
  script::{codegen_module, parse_module},
  swc_atoms::AtomStore,
  swc_ecma_visit::{VisitMut, VisitMutWith},
};

use farmfe_macro_plugin::farm_plugin;

#[farm_plugin]
pub struct FarmPluginResourcesImportMeta {}

impl FarmPluginResourcesImportMeta {
  fn new(_config: &Config, _options: String) -> Self {
    Self {}
  }
}

#[derive(Debug, Clone)]
pub struct MinimalResourceInfo {
  pub name: String,
  pub resource_type: ResourceType,
}

impl Plugin for FarmPluginResourcesImportMeta {
  fn name(&self) -> &str {
    "FarmPluginResourcesImportMeta"
  }

  fn priority(&self) -> i32 {
    101
  }

  fn finalize_resources(
    &self,
    param: &mut PluginFinalizeResourcesHookParams,
    _context: &Arc<CompilationContext>,
  ) -> Result<Option<()>> {
    let mut resources = Vec::new();
    for (name, resource) in &mut param.resources_map.iter() {
      resources.push(MinimalResourceInfo {
        name: name.clone(),
        resource_type: resource.resource_type.clone(),
      });
    }

    for resource in &mut param.resources_map.values_mut() {
      if let ResourceType::Js = resource.resource_type {
        let mut module = parse_module(
          resource.name.as_str(),
          str::from_utf8(resource.bytes.as_slice()).unwrap(),
          Default::default(),
          Default::default(),
        )?;

        module.ast.visit_mut_with(&mut ImportMetaVisitor {
          resources: &resources,
          atom_store: AtomStore::default(),
        });

        let cm = Arc::new(SourceMap::default());

        resource.bytes = codegen_module(
          &module.ast,
          param.config.script.target,
          cm,
          None,
          param.config.minify.enabled(),
          None,
        )
        .unwrap();
      }
    }

    Ok(None)
  }
}

struct ImportMetaVisitor<'a> {
  resources: &'a Vec<MinimalResourceInfo>,
  atom_store: AtomStore,
}

impl<'a> ImportMetaVisitor<'a> {
  fn get_resource_expr(resource: &MinimalResourceInfo) -> Expr {
    return Expr::Object(ObjectLit {
      span: Default::default(),
      props: vec![
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
          key: PropName::Ident(Ident::from("name")),
          value: Box::new(Expr::Lit(Lit::from(resource.name.clone()))),
        }))),
        PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
          key: PropName::Ident(Ident::from("type")),
          value: Box::new(Expr::Lit(Lit::from(resource.resource_type.to_string()))),
        }))),
      ],
    });
  }

  fn get_resources_expr<'b>(resources: &mut impl Iterator<Item = &'b MinimalResourceInfo>) -> Expr {
    let mut elems = Vec::new();
    for resource in resources {
      elems.push(Some(ExprOrSpread {
        spread: None,
        expr: Box::new(Self::get_resource_expr(resource)),
      }));
    }

    return Expr::Array(ArrayLit {
      span: Default::default(),
      elems,
    });
  }

  fn is_resources_import_meta(expr: &Expr) -> bool {
    match expr {
      Expr::Member(member) => match member.obj.as_ref() {
        Expr::MetaProp(meta_prop) => {
          if matches!(meta_prop.kind, MetaPropKind::ImportMeta) {
            if let MemberProp::Ident(member_prop) = &member.prop {
              if member_prop.sym.eq("resources") {
                return true;
              }
            }
          }
        }
        _ => {}
      },
      _ => {}
    }

    return false;
  }
}

impl<'a> VisitMut for ImportMetaVisitor<'a> {
  fn visit_mut_expr(&mut self, expr: &mut Expr) {
    match expr {
      Expr::Member(member) => {
        if let Expr::Member(inner_member) = member.obj.as_ref() {
          if Self::is_resources_import_meta(inner_member.obj.as_ref()) {
            match &inner_member.prop {
              MemberProp::Ident(ident) => match &member.prop {
                MemberProp::Computed(computed) => {
                  // `import.meta.resources.xxx[n]`
                  if let Expr::Lit(Lit::Num(num)) = computed.expr.as_ref() {
                    if num.value.floor() == num.value {
                      // this is a whole number so we can use it as an index
                      let resource_type = ident.sym.to_string();
                      if let Some(resource) = self
                        .resources
                        .iter()
                        .filter(|r| r.resource_type.to_string() == resource_type)
                        .nth(num.value as usize)
                      {
                        *expr = Self::get_resource_expr(resource);
                        return;
                      }
                    }
                  }
                }
                _ => {}
              },
              _ => {}
            }
          }
        }

        if Self::is_resources_import_meta(member.obj.as_ref()) {
          match &member.prop {
            MemberProp::Ident(ident) => {
              // `import.meta.resources.xxx`
              let resource_type = ident.sym.to_string();
              let filtered_resources = self
                .resources
                .iter()
                .filter(|r| r.resource_type.to_string() == resource_type)
                .collect::<Vec<_>>();
              if filtered_resources.len() > 0 {
                *expr = Self::get_resources_expr(&mut filtered_resources.into_iter());
                return;
              }
            }
            MemberProp::Computed(computed) => {
              // `import.meta.resources[n]`
              if let Expr::Lit(Lit::Num(num)) = computed.expr.as_ref() {
                if num.value.floor() == num.value {
                  // this is a whole number so we can use it as an index
                  if let Some(resource) = self.resources.iter().nth(num.value as usize) {
                    *expr = Self::get_resource_expr(resource);
                    return;
                  }
                }
              }
            }
            _ => {}
          }
        }
      }
      _ => {}
    }

    if Self::is_resources_import_meta(expr) {
      // `import.meta.resources`
      // replace with the array literal Object.assign([...], { css: [...], js: [...], ... })
      *expr = Expr::Call(CallExpr {
        span: Default::default(),
        type_args: None,
        callee: Callee::Expr(Box::new(Expr::Member(MemberExpr {
          span: Default::default(),
          obj: Box::new(Expr::Ident(Ident::from("Object"))),
          prop: MemberProp::Ident(Ident::from("assign")),
        }))),
        args: vec![
          ExprOrSpread {
            spread: None,
            expr: Box::new(Self::get_resources_expr(&mut self.resources.iter())),
          },
          ExprOrSpread {
            spread: None,
            expr: Box::new(Expr::Object(ObjectLit {
              span: Default::default(),
              props: {
                let mut resources_map = HashMap::<String, Vec<&'a MinimalResourceInfo>>::new();
                for resources in self.resources {
                  let entry = resources_map
                    .entry(resources.resource_type.to_string())
                    .or_insert_with(|| Vec::new());
                  entry.push(resources);
                }

                let mut props = Vec::new();
                for (resource_type, resources) in resources_map.into_iter() {
                  props.push(PropOrSpread::Prop(Box::new(Prop::KeyValue(KeyValueProp {
                    key: PropName::Ident(Ident {
                      span: Default::default(),
                      optional: false,
                      sym: self.atom_store.atom(resource_type),
                    }),
                    value: Box::new(Self::get_resources_expr(&mut resources.into_iter())),
                  }))))
                }

                props
              },
            })),
          },
        ],
      });
      return;
    }

    expr.visit_mut_children_with(self);
  }
}
