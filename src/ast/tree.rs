//! [`Ast`], the owner of a whole build's surface syntax.
//!
//! The parser works one file at a time, so a build starts life as a `Vec<ParsedSrcFile>`: each
//! file's own module header, its imports, and its definitions, still separate from every other
//! file's. [`Ast::new`] is what turns that into the program's module tree, merging every file
//! that declares into the same module and synthesizing any module -- `math`, when only
//! `math::vector` is ever declared -- that no file names on its own.
//!
//! [`Ast`] is to the AST what [`Hir`](crate::hir::Hir) is to the HIR: the root that owns every
//! node, records each module's parent, and names the root module. The nodes below a module are
//! unchanged -- still the parser's `Box`-linked [`Item`]s, addressed by following the tree rather
//! than by id.

use std::collections::HashMap;

use crate::ast::{Ident, Import, Item, ParsedSrcFile, Path, Symbol};
use crate::driver::source::SrcSpan;

/// Identifies one module in an [`Ast`]. Modules are numbered densely from the root, and a
/// module's ancestors are always numbered before it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct ModId(u32);

impl ModId {
    fn from_usize(index: usize) -> Self {
        ModId(index as u32)
    }

    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// One module's surface syntax, gathered from every file that declares into it.
#[derive(Debug)]
pub struct AstModule {
    /// The module's dotted path from the root, which is empty for the root itself.
    pub path: Path,
    pub imports: Vec<Import>,
    pub items: Vec<Item>,
    pub children: Vec<ModId>,
}

/// The whole build's syntax: a tree of modules, each owning the items and imports of every file
/// that declares into it.
#[derive(Debug)]
pub struct Ast {
    /// Every module in the program, indexed by its [`ModId`]. A module's ancestors always sit at
    /// lower indices than it, since [`AstBuilder::module_for_path`] creates a module only after
    /// the module above it exists. Passes that allocate as they walk this rely on it.
    modules: Vec<AstModule>,

    /// Maps each module, indexed by its [`ModId`], to the module it is declared inside.
    ///
    /// The root is its own parent, which keeps this dense -- a `Vec<ModId>` rather than a
    /// `Vec<Option<ModId>>` -- while leaving "has no parent" representable, exactly as
    /// [`Hir::parents`](crate::hir::Hir) does. [`Ast::parent`] turns that back into an `Option`.
    parents: Vec<ModId>,

    root: ModId,
}

impl Ast {
    /// Collects every parsed file of a build into one module tree.
    ///
    /// A file's items and imports go to the module its `module a::b;` header names, or to the
    /// root if it has none. Files are consumed in the order given, which is `SrcMap` order, so
    /// the merged item order is reproducible.
    pub fn new(files: Vec<ParsedSrcFile>) -> Ast {
        let mut builder = AstBuilder::new();
        for file in files {
            let module = match &file.module {
                Some(decl) => builder.module_for_path(&decl.path.segments),
                None => builder.ast.root,
            };
            let target = &mut builder.ast.modules[module.index()];
            target.imports.extend(file.imports);
            // `Parser::assemble_file` has already sorted this file's `module` header and its
            // imports out of `items`, so what is left is definitions.
            target.items.extend(file.items);
        }
        builder.ast
    }

    pub fn root(&self) -> &AstModule {
        self.module(self.root)
    }

    pub fn root_id(&self) -> ModId {
        self.root
    }

    pub fn module(&self, id: ModId) -> &AstModule {
        &self.modules[id.index()]
    }

    /// Returns the module `id` is declared inside, or `None` if `id` names the root.
    ///
    /// The root is stored as its own parent, which is how the table stays dense. Reporting that
    /// as `None` is what gives a caller walking upwards a termination condition.
    pub fn parent(&self, id: ModId) -> Option<ModId> {
        let parent = self.parents[id.index()];
        (parent != id).then_some(parent)
    }

    /// Iterates every module, parents before children.
    pub fn mod_ids(&self) -> impl Iterator<Item = ModId> + '_ {
        (0..self.modules.len()).map(ModId::from_usize)
    }
}

/// Builds an [`Ast`]'s module tree, keeping the path index that only construction needs.
struct AstBuilder {
    ast: Ast,
    /// Module path (dotted segments) -> its [`ModId`], populated as modules are discovered --
    /// either from an explicit `module a::b;` header or synthesized as an ancestor of one.
    by_path: HashMap<Vec<Symbol>, ModId>,
}

impl AstBuilder {
    fn new() -> Self {
        let root = ModId::from_usize(0);
        let ast = Ast {
            modules: vec![AstModule {
                path: Path {
                    segments: Vec::new(),
                    span: SrcSpan::new(0, 0),
                },
                imports: Vec::new(),
                items: Vec::new(),
                children: Vec::new(),
            }],
            parents: vec![root],
            root,
        };

        AstBuilder {
            ast,
            by_path: HashMap::from([(Vec::new(), root)]),
        }
    }

    /// Finds or creates the module named by `segments`, synthesizing any ancestor module no file
    /// declares on its own.
    fn module_for_path(&mut self, segments: &[Ident]) -> ModId {
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
            let id = ModId::from_usize(self.ast.modules.len());
            self.ast.modules.push(AstModule {
                path: Path {
                    segments: path_segments,
                    span,
                },
                imports: Vec::new(),
                items: Vec::new(),
                children: Vec::new(),
            });
            self.ast.parents.push(current);
            self.ast.modules[current.index()].children.push(id);
            self.by_path.insert(prefix.clone(), id);
            current = id;
        }
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ModuleDecl;
    use crate::ast::interner::Interner;
    use crate::testing::parse_src;

    /// Attaches a `module a::b;` header to a parsed file by hand. The parser doesn't currently
    /// wire a file's header into [`ParsedSrcFile::module`], so this is what exercises the tree
    /// building below it.
    fn with_header(mut file: ParsedSrcFile, segments: &[&str]) -> ParsedSrcFile {
        let span = file.span;
        file.module = Some(ModuleDecl {
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

    fn path_of(ast: &Ast, id: ModId) -> Vec<String> {
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
