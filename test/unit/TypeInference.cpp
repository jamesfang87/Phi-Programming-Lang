#include <gtest/gtest.h>
#include <memory>
#include <optional>
#include <sstream>
#include <string>
#include <unordered_map>
#include <vector>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Driver/Driver.hpp"
#include "SrcManager/SrcManager.hpp"

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"

using namespace phi;

class TypeInferenceTest : public ::testing::Test {
protected:
  void SetUp() override {
    SrcMan = std::make_shared<SrcManager>();
    DiagMan = std::make_shared<DiagnosticManager>(SrcMan);
  }

  // wrapper for compileToAST()
  std::optional<std::vector<std::unique_ptr<Decl>>>
  compileToASTWrap(const std::string &Code) {
    std::string FilePath = "test.phi";
    SrcMan->addSrcFile(FilePath, Code);

    PhiCompiler Compiler(Code, FilePath, DiagMan);
    return Compiler.compileToAST();
  }

  using NameTypeMap = std::unordered_map<std::string, std::string>;

  // Collect id/type from any ValueDecl (VarDecl, ParamDecl, FieldDecl, etc.)
  void collectFromDecl(Decl *D, NameTypeMap &out) {
    if (!D)
      return;

    // Many concrete decls derive from ValueDecl and expose getId(), hasType(),
    // getType()
    if (auto V = dynamic_cast<ValueDecl *>(D)) {
      std::string id = V->getId();
      if (!id.empty()) {
        if (V->hasType())
          out.emplace(id, V->getType().toString());
        else
          out.emplace(id, "<unresolved>");
      }
    }

    // VarDecl: traverse initializer if present
    if (auto VD = dynamic_cast<VarDecl *>(D)) {
      if (VD->hasInit())
        collectFromExpr(&VD->getInit(), out);
      return;
    }

    // FieldDecl: traverse initializer
    if (auto FD = dynamic_cast<FieldDecl *>(D)) {
      if (FD->hasInit())
        collectFromExpr(&FD->getInit(), out);
      return;
    }

    // ParamDecl handled via ValueDecl branch above

    // FunDecl: collect params and traverse body
    if (auto FD = dynamic_cast<FunDecl *>(D)) {
      // params: many examples in your .cpp iterate over FD->Params; try both
      // getter and member Prefer getParams() if exposed; otherwise fall back to
      // member access (compiler header determines which compiles)
      for (auto &Pptr : FD->getParams()) {
        if (!Pptr)
          continue;
        const ParamDecl *P = Pptr.get();
        if (!P->getId().empty()) {
          if (P->hasType())
            out.emplace(P->getId(), P->getType().toString());
          else
            out.emplace(P->getId(), "<unresolved>");
        }
      }

      // body
      collectFromBlock(&FD->getBody(), out);
      return;
    }

    // StructDecl: fields & methods
    if (auto SD = dynamic_cast<StructDecl *>(D)) {
      for (const auto &FldPtr : SD->getFields()) {
        if (!FldPtr)
          continue;
        const FieldDecl *Fld = FldPtr.get();
        if (!Fld->getId().empty()) {
          if (Fld->hasType())
            out.emplace(Fld->getId(), Fld->getType().toString());
          else
            out.emplace(Fld->getId(), "<unresolved>");
        }
        if (Fld->hasInit())
          collectFromExpr(&Fld->getInit(), out);
      }
      // methods: traverse bodies if present
      for (const auto &M : SD->getMethods()) {
        collectFromBlock(&M.getBody(), out);
      }
      return;
    }
  }

  void collectFromBlock(Block *B, TypeInferenceTest::NameTypeMap &out) {
    if (!B)
      return;

    for (const auto &StmtPtr : B->getStmts()) {
      if (!StmtPtr)
        continue;
      collectFromStmt(StmtPtr.get(), out);
    }
  }

  void collectFromStmt(Stmt *S, NameTypeMap &out) {
    if (!S)
      return;

    if (auto DS = dynamic_cast<DeclStmt *>(S)) {
      collectFromDecl(&DS->getDecl(), out);
      return;
    }

    if (auto ES = dynamic_cast<ExprStmt *>(S)) {
      collectFromExpr(&ES->getExpr(), out);
      return;
    }

    if (auto IF = dynamic_cast<IfStmt *>(S)) {
      collectFromExpr(&IF->getCond(), out);
      collectFromBlock(&IF->getThen(), out);
      if (IF->hasElse())
        collectFromBlock(&IF->getElse(), out);
      return;
    }

    if (auto FS = dynamic_cast<ForStmt *>(S)) {
      collectFromDecl(&FS->getLoopVar(), out);
      collectFromExpr(&FS->getRange(), out);
      collectFromBlock(&FS->getBody(), out);
      return;
    }

    if (auto WS = dynamic_cast<WhileStmt *>(S)) {
      collectFromExpr(&WS->getCond(), out);
      collectFromBlock(&WS->getBody(), out);
      return;
    }

    if (auto RS = dynamic_cast<const ReturnStmt *>(S)) {
      if (RS->hasExpr())
        collectFromExpr(&RS->getExpr(), out);
      return;
    }

    // add other statement kinds if you have them
  }

  void collectFromExpr(Expr *E, NameTypeMap &out) {
    if (!E)
      return;

    if (auto B = dynamic_cast<BinaryOp *>(E)) {
      collectFromExpr(&B->getLhs(), out);
      collectFromExpr(&B->getRhs(), out);
      return;
    }

    if (auto DR = dynamic_cast<DeclRefExpr *>(E)) {
      if (DR->getDecl())
        collectFromDecl(DR->getDecl(), out);
      return;
    }

    if (auto FC = dynamic_cast<FunCallExpr *>(E)) {
      collectFromExpr(&FC->getCallee(), out);
      for (const auto &ArgPtr : FC->getArgs()) {
        if (!ArgPtr)
          continue;
        collectFromExpr(ArgPtr.get(), out);
      }
      if (FC->getDecl())
        collectFromDecl(FC->getDecl(), out);
      return;
    }

    if (auto SL = dynamic_cast<StructLiteral *>(E)) {
      for (const auto &FI : SL->getFields()) {
        collectFromExpr(FI->getInitValue(), out);
      }
      return;
    }

    if (auto RL = dynamic_cast<RangeLiteral *>(E)) {
      collectFromExpr(&RL->getStart(), out);
      collectFromExpr(&RL->getEnd(), out);
      return;
    }

    if (auto TL = dynamic_cast<TupleLiteral *>(E)) {
      for (const auto &Elem : TL->getElements()) {
        if (Elem)
          collectFromExpr(Elem.get(), out);
      }
      return;
    }

    // literal nodes and other nodes are leaves for our collection
  }

  NameTypeMap collectTopLevel(const std::vector<std::unique_ptr<Decl>> &AST) {
    NameTypeMap out;
    for (const auto &UD : AST) {
      if (!UD)
        continue;
      collectFromDecl(UD.get(), out);

      // If the top-level decl is a function, walk its body as well (some code
      // paths above also do)
      if (auto FD = dynamic_cast<const FunDecl *>(UD.get())) {
        collectFromBlock(&FD->getBody(), out);
      }

      // If struct, fields & methods handled in collectFromDecl
    }
    return out;
  }

  std::string mapToString(const NameTypeMap &m) const {
    std::ostringstream oss;
    for (const auto &p : m)
      oss << p.first << ":" << p.second << "; ";
    return oss.str();
  }

  std::shared_ptr<SrcManager> SrcMan;
  std::shared_ptr<DiagnosticManager> DiagMan;
};

// ---------------- TESTS ----------------

TEST_F(TypeInferenceTest, DefaultsAndExplicitTypes) {
  const std::string Code = R"(
    fun main() {
      var a = 42;
      var b = 3.14;
      var c = true;
      var s = "hi";
      const big: i64 = 123;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);

  EXPECT_EQ(Map["a"], "i32");
  EXPECT_EQ(Map["b"], "f32");
  EXPECT_EQ(Map["c"], "bool");
  EXPECT_EQ(Map["s"], "string");
  EXPECT_EQ(Map["big"], "i64");
}

TEST_F(TypeInferenceTest, NoImplicitNumericCoercion) {
  const std::string Code = R"(
    fun main() {
      var i = 1;
      var f = 1.0;
      var m = i + f; // should not coerce implicitly
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, LoopsShadowingAndLocals) {
  const std::string Code = R"(
    fun main() {
      var outer = 0;
      for idx in 0..3 {
        var inner = idx;
      }

      while outer < 10 {
        var tmp = outer;
        outer = outer + 1;
      }
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["outer"], "i32");
  EXPECT_EQ(Map["idx"], "i32");
  EXPECT_TRUE(Map.find("inner") != Map.end());
  EXPECT_EQ(Map["tmp"], "i32");
}

TEST_F(TypeInferenceTest, FunctionParamsAndLocals) {
  const std::string Code = R"(
    fun add(const a: i32, const b: i32) -> i32 {
      const sum = a + b;
      return sum;
    }

    fun caller() {
      var r = add(1, 2);
      const c = 10;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());
  auto Map = collectTopLevel(*MaybeAst);

  EXPECT_EQ(Map["a"], "i32");
  EXPECT_EQ(Map["b"], "i32");
  EXPECT_EQ(Map["sum"], "i32");
  EXPECT_EQ(Map["r"], "i32");
  EXPECT_EQ(Map["c"], "i32");
}

TEST_F(TypeInferenceTest, StructFieldsAndInitializers) {
  const std::string Code = R"(
    struct Point {
      x: i32;
      y: i32;
    }

    fun make() {
      var p = Point { x = 1, y = 2 };
      var px = p.x;
      var py = p.y;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());
  auto Map = collectTopLevel(*MaybeAst);

  EXPECT_EQ(Map["x"], "i32");
  EXPECT_EQ(Map["y"], "i32");
  EXPECT_EQ(Map["p"], "Point");
  EXPECT_EQ(Map["px"], "i32");
  EXPECT_EQ(Map["py"], "i32");
}

TEST_F(TypeInferenceTest, ComplexControlFlowAndLocals) {
  const std::string Code = R"(
    fun compute() -> i32 {
      var acc = 0;
      for i in 0..5 {
        if i == 2 {
          continue;
        }
        var t = i * 2;
        acc = acc + t;
      }

      if acc > 5 {
        var extra = 100;
        return acc + extra;
      }
      return acc;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());
  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["acc"], "i32");
  EXPECT_EQ(Map["i"], "i32");
  EXPECT_EQ(Map["t"], "i32");
  EXPECT_EQ(Map["extra"], "i32");
}

TEST_F(TypeInferenceTest, TypeErrorDetectedForMismatchedBinaryOp) {
  const std::string Code = R"(
    fun main() {
      var a = 1;
      var b = "nope";
      var z = a + b; // type error
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());

  // Expect at least one diagnostic error recorded for the type mismatch
  EXPECT_GT(DiagMan->error_count(), 0) << "Expected type mismatch diagnostics";
}

TEST_F(TypeInferenceTest, StructExprs) {
  const std::string Code = R"(
    struct RGB {
        r: i32;
        b: i32;
        g: i32;

        public fun compareRed(const this, const other: i32) -> i32 {
            return this.r - other;
        }
    }

    fun main() {
        const color = RGB { r = 255, b = 255, g = 255};
        const foo = color.r;
        const bar = color.compareRed(7);
    }
    )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());
  auto Map = collectTopLevel(*MaybeAst);

  EXPECT_EQ(Map["foo"], "i32");
  EXPECT_EQ(Map["bar"], "i32");
  EXPECT_EQ(Map["color"], "RGB");
}

TEST_F(TypeInferenceTest, InferenceWithStatements) {
  const std::string Code = R"(
    fun foo(var foo: i32, var bar: i64) {
        foo + 10;
        bar + 20;
    }

    fun main() {
        var x = 10;
        var y = 1 * 9 + 8;

        const a: f64 = 1.2;
        const b = a;
        const c = 1.6;

        for i in 1..10 {
            const t: u64 = i;
        }

        foo(x, y);
    }
    )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());
  auto Map = collectTopLevel(*MaybeAst);

  EXPECT_EQ(Map["foo"], "i32");
  EXPECT_EQ(Map["bar"], "i64");
  EXPECT_EQ(Map["x"], "i32");
  EXPECT_EQ(Map["y"], "i64");
  EXPECT_EQ(Map["a"], "f64");
  EXPECT_EQ(Map["b"], "f64");
  EXPECT_EQ(Map["c"], "f32");
  EXPECT_EQ(Map["i"], "u64");
}

TEST_F(TypeInferenceTest, InferenceErrorWithStatements) {
  const std::string Code = R"(
    fun foo(var foo: i32, var bar: i64) {
        foo + 10;
        bar + 20;
    }

    fun main() {
        var x = 10;
        var y = 1 * 9 + 8;

        const a: f64 = 1.2;
        const b = a;
        const c = 1.6;

        for i in 1..10 {
            const t: f64 = i;
        }

        foo(x, y);
    }
    )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, AssignmentTypeMismatch) {
  const std::string Code = R"(
    fun main() {
      var x: i32 = 1.5; // assigning float to i32
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, BinaryOpTypeMismatch) {
  const std::string Code = R"(
    fun main() {
      var a = 1;
      var b = true;
      var c = a + b; // cannot add int and bool
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, StructFieldTypeMismatch) {
  const std::string Code = R"(
    struct Point {
      x: i32;
      y: i32;
    }

    fun main() {
      var p = Point { x = 1, y = 2.5 }; // y expects i32 but got float
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, FunctionCallArgumentMismatch) {
  const std::string Code = R"(
    fun sum(const a: i32, const b: i32) -> i32 {
      return a + b;
    }

    fun main() {
      var r = sum(1, 2.5); // second argument is float, expects i32
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, IncompatibleReturnType) {
  const std::string Code = R"(
    fun foo() -> i32 {
      return 1.5; // returning float for i32 function
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, TupleElementTypeMismatch) {
  const std::string Code = R"(
    fun main() {
      var t = (1, true + 3); // bool + int invalid
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, StructMethodCallTypeError) {
  const std::string Code = R"(
    struct RGB {
        r: i32;
        g: i32;
        b: i32;

        fun addRed(const this, const val: i32) -> i32 {
            return this.r + val;
        }
    }

    fun main() {
        var color = RGB { r = 1, g = 2, b = 3 };
        var result = color.addRed("oops"); // argument type mismatch
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, FunctionCallWithStructParam) {
  const std::string Code = R"(
    struct Pair {
        public first: i32;
        public second: i32;
    }

    fun sum(const pair: Pair) -> i32 {
        return pair.first + pair.second;
    }

    fun main() {
        const pair = Pair {first = 10, second = 20};
        var result = sum(pair);
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["result"], "i32"); // inferred from add return
}

TEST_F(TypeInferenceTest, NestedFunctionCalls) {
  const std::string Code = R"(
    fun square(const x: i32) -> i32 {
        return x * x;
    }

    fun double(const x: i32) -> i32 {
        return x + x;
    }

    fun main() {
        var x = double(square(3));
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["x"], "i32"); // should infer correctly through nested calls
}

TEST_F(TypeInferenceTest, FunctionCallWithIncorrectArgumentType) {
  const std::string Code = R"(
    fun increment(const x: i32) -> i32 {
        return x + 1;
    }

    fun main() {
        var a = increment(2.5); // float passed to i32 param
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_FALSE(MaybeAst.has_value());
}

TEST_F(TypeInferenceTest, FunctionCallReturnUsedInExpression) {
  const std::string Code = R"(
    fun getValue() -> i32 {
        return 10;
    }

    fun main() {
        var x = getValue() * 2;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["x"], "i32");
}

/*
TEST_F(TypeInferenceTest, FunctionCallWithStructReturn) {
  const std::string Code = R"(
    struct Point {
      x: i32;
      y: i32;
    }

    fun makePoint(a: i32, b: i32) -> Point {
        return Point { x = a, y = b }
    }

    fun main() {
        var p = makePoint(1, 2);
        var px = p.x;
        var py = p.y;
    }
  )";

  auto MaybeAst = compileToASTWrap(Code);
  ASSERT_TRUE(MaybeAst.has_value());

  auto Map = collectTopLevel(*MaybeAst);
  EXPECT_EQ(Map["p"], "Point");
  EXPECT_EQ(Map["px"], "i32");
  EXPECT_EQ(Map["py"], "i32");
}
*/
