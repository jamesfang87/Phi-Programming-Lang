//! [`AstBuilder`] which turns a build's parsed files into the [`Ast`] module tree.

use std::collections::HashMap;

use crate::ast::{Ast, Ident, Module, NodeId, Path, Symbol};
use crate::driver::source::SrcSpan;

/// Builds an [`Ast`]'s module tree, keeping the path index that only construction needs.
pub(super) struct AstBuilder {
    pub(super) ast: Ast,
    /// Module path -> its [`NodeId`]
    by_path: HashMap<Vec<Symbol>, NodeId>,
}

impl AstBuilder {
    pub(super) fn new() -> Self {
        let root = NodeId::next();
        let ast = Ast {
            modules: vec![Module {
                id: root,
                path: Path {
                    segments: Vec::new(),
                    span: SrcSpan::new(0, 0),
                },
                imports: Vec::new(),
                items: Vec::new(),
                children: Vec::new(),
            }],
            module_positions: HashMap::from([(root, 0)]),
            parent_module: vec![root],
            root,
        };

        AstBuilder {
            ast,
            by_path: HashMap::from([(Vec::new(), root)]),
        }
    }

    pub(super) fn module_for_path(&mut self, segments: &[Ident]) -> NodeId {
        let mut current = self.ast.root;
        let mut prefix: Vec<Symbol> = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            prefix.push(seg.text);
            if let Some(&existing) = self.by_path.get(&prefix) {
                current = existing;
                continue;
            }
            // `current` is still the module one level up: the parent of the one being created.
            let path_segments = segments[..=i].to_vec();
            let span = path_segments[0]
                .span
                .merge(path_segments[path_segments.len() - 1].span);
            let id = NodeId::next();
            let position = self.ast.modules.len();
            self.ast.modules.push(Module {
                id,
                path: Path {
                    segments: path_segments,
                    span,
                },
                imports: Vec::new(),
                items: Vec::new(),
                children: Vec::new(),
            });
            self.ast.parent_module.push(current);
            self.ast.module_positions.insert(id, position);
            let current_position = self.ast.module_positions[&current];
            self.ast.modules[current_position].children.push(id);
            self.by_path.insert(prefix.clone(), id);
            current = id;
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::interner::Interner;
    use crate::ast::{ModuleDecl, NodeId, ParsedSrcFile};
    use crate::testing::parse_src;

    /// Attaches a `module a::b;` header to a parsed file by hand. The parser doesn't currently
    /// wire a file's header into [`ParsedSrcFile::module`], so this is what exercises the tree
    /// building below it.
    fn with_header(mut file: ParsedSrcFile, segments: &[&str]) -> ParsedSrcFile {
        let span = file.span;
        file.module = Some(ModuleDecl {
            id: NodeId::next(),
            path: Path {
                segments: segments
                    .iter()
                    .map(|s| Ident {
                        text: Interner::intern(s),
                        span,
                    })
                    .collect(),
                span,
            },
            span,
        });
        file
    }

    fn path_of(ast: &Ast, id: NodeId) -> Vec<String> {
        ast.module(id)
            .path
            .segments
            .iter()
            .map(|seg| Interner::resolve(seg.text).to_string())
            .collect()
    }

    #[test]
    fn a_file_without_a_header_lands_in_the_root() {
        let ast = Ast::new(vec![parse_src("fun main() {}")]);

        assert_eq!(ast.mod_ids().count(), 1);
        assert_eq!(ast.root().items.len(), 1);
        assert!(ast.root().children.is_empty());
        assert_eq!(ast.parent(ast.root_id()), None);
    }

    #[test]
    fn a_nested_header_synthesizes_its_ancestors() {
        let ast = Ast::new(vec![with_header(
            parse_src("fun helper() {}"),
            &["math", "vector"],
        )]);

        assert_eq!(ast.mod_ids().count(), 3);
        assert!(ast.root().items.is_empty());
        assert_eq!(ast.root().children.len(), 1);

        let math = ast.root().children[0];
        assert_eq!(path_of(&ast, math), ["math"]);
        assert_eq!(ast.parent(math), Some(ast.root_id()));
        assert!(ast.module(math).items.is_empty());

        let vector = ast.module(math).children[0];
        assert_eq!(path_of(&ast, vector), ["math", "vector"]);
        assert_eq!(ast.parent(vector), Some(math));
        assert_eq!(ast.module(vector).items.len(), 1);
    }

    #[test]
    fn two_files_declaring_one_module_are_merged() {
        // One parse cloned into two files: `parse_src` clears the interner, so parsing twice in
        // one test would leave the first file's symbols naming whatever was interned after it.
        let file = parse_src("import a::b; fun first() {}");
        let ast = Ast::new(vec![
            with_header(file.clone(), &["math"]),
            with_header(file, &["math"]),
        ]);

        assert_eq!(ast.mod_ids().count(), 2);
        let math = ast.root().children[0];
        let module = ast.module(math);
        assert_eq!(module.imports.len(), 2);
        assert_eq!(module.items.len(), 2);
    }

    #[test]
    fn sibling_modules_share_one_synthesized_ancestor() {
        let file = parse_src("fun b() {}");
        let ast = Ast::new(vec![
            with_header(file.clone(), &["math", "b"]),
            with_header(file, &["math", "c"]),
        ]);

        assert_eq!(ast.mod_ids().count(), 4);
        let math = ast.root().children[0];
        assert_eq!(ast.module(math).children.len(), 2);
        for child in &ast.module(math).children {
            assert_eq!(ast.parent(*child), Some(math));
        }
    }

    /// A module declared explicitly by one file and implicitly, as an ancestor, by another is one
    /// module either way around -- whichever file the build happens to reach first.
    #[test]
    fn an_ancestor_declared_by_its_own_file_is_not_duplicated() {
        let file = parse_src("fun a() {}");
        let declared_first = Ast::new(vec![
            with_header(file.clone(), &["math"]),
            with_header(file.clone(), &["math", "vector"]),
        ]);
        let declared_second = Ast::new(vec![
            with_header(file.clone(), &["math", "vector"]),
            with_header(file, &["math"]),
        ]);

        for ast in [&declared_first, &declared_second] {
            assert_eq!(ast.mod_ids().count(), 3);
            let math = ast.root().children[0];
            assert_eq!(ast.module(math).items.len(), 1);
            assert_eq!(ast.module(math).children.len(), 1);
        }
    }
}
