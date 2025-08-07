#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Base class for all declaration nodes in the AST
 *
 * Represents named entities in the source code like variables, parameters, and
 * functions. Contains common properties including source location, identifier,
 * and type information.
 */
class Decl {
public:
  Decl(SrcLocation Location, std::string Id, Type DeclType)
      : Location(std::move(Location)), Id(std::move(Id)),
        DeclType(std::move(DeclType)) {}

  virtual ~Decl() = default;

  [[nodiscard]] std::string getId() { return Id; }
  [[nodiscard]] Type getType() { return *DeclType; }
  [[nodiscard]] virtual bool isConst() const = 0;

  virtual void emit(int level) const = 0;

protected:
  SrcLocation Location;         ///< Source location of declaration
  std::string Id;               ///< Name of declared entity
  std::optional<Type> DeclType; ///< Type information (std::nullopt for
                                ///< unresolved declarations)
};

/**
 * @brief Variable declaration node
 *
 * Represents variable declarations including:
 * - Constant vs mutable status
 * - Optional initializer expression
 * - Type annotation
 */
class VarDecl final : public Decl {
public:
  /**
   * @brief Constructs a VarDecl node
   * @param location Source location of declaration
   * @param identifier Variable name
   * @param type Variable type
   * @param isConst Constant flag (true for const)
   * @param initializer Initial value expression
   */
  VarDecl(SrcLocation Location, std::string Id, Type DeclType,
          const bool IsConst, std::unique_ptr<Expr> Init)
      : Decl(std::move(Location), std::move(Id), std::move(DeclType)),
        IsConst(IsConst), Init(std::move(Init)) {}

  [[nodiscard]] bool isConst() const override { return IsConst; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }
  [[nodiscard]] Expr &getInit() const { return *Init; }

  void emit(int level) const override;

private:
  const bool IsConst;         ///< Constant declaration flag
  std::unique_ptr<Expr> Init; ///< Initial value expression
};

/**
 * @brief Function parameter declaration node
 *
 * Represents formal parameters in function declarations. Contains parameter
 * name and type information.
 */
class ParamDecl final : public Decl {
public:
  /**
   * @brief Constructs a ParamDecl node
   * @param location Source location of parameter
   * @param identifier Parameter name
   * @param type Parameter type
   */
  ParamDecl(SrcLocation Location, std::string Id, Type DeclType, bool IsConst)
      : Decl(std::move(Location), std::move(Id), std::move(DeclType)),
        IsConst(IsConst) {}

  [[nodiscard]] bool isConst() const override { return IsConst; }

  void emit(int level) const override;

private:
  const bool IsConst = false;
};

/**
 * @brief Function declaration node
 *
 * Represents function definitions containing:
 * - Return type
 * - Parameter list
 * - Function body block
 */
class FunDecl final : public Decl {
public:
  /**
   * @brief Constructs a FunDecl node
   * @param location Source location of function
   * @param identifier Function name
   * @param return_type Function return type
   * @param params List of function parameters
   * @param block_ptr Function body block
   */
  FunDecl(SrcLocation Location, std::string Id, Type ReturnType,
          std::vector<std::unique_ptr<ParamDecl>> Params,
          std::unique_ptr<Block> BlockPtr)
      : Decl(std::move(Location), std::move(Id), std::move(ReturnType)),
        Params(std::move(Params)), Block(std::move(BlockPtr)) {}

  [[nodiscard]] bool isConst() const override { return false; }
  [[nodiscard]] Type getReturnTy() const { return DeclType.value(); }
  [[nodiscard]] Block &getBlock() const { return *Block; }
  [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>> &getParams() {
    return Params;
  }
  void setBlock(std::unique_ptr<Block> BPtr) { Block = std::move(BPtr); }

  void emit(int level) const override;

private:
  std::vector<std::unique_ptr<ParamDecl>> Params; ///< Function parameters
  std::unique_ptr<Block> Block;                   ///< Function body statements
};

// class StructDecl final : public Decl {
// public:
//   /**
//    * @brief Constructs a StructDecl node
//    * @param location Source location of struct
//    * @param identifier Struct name
//    * @param fields List of struct fields
//    */
//   StructDecl(SrcLocation Location, std::string Id,
//              std::vector<std::unique_ptr<FieldDecl>> Fields)
//       : Decl(std::move(Location), std::move(Id), Type::VOID),
//         Fields(std::move(Fields)) {}

//   [[nodiscard]] bool isConst() const override { return false; }
//   [[nodiscard]] std::vector<std::unique_ptr<FieldDecl>> &getFields() {
//     return Fields;
//   }

//   void emit(int level) const override;

// private:
//   std::vector<std::unique_ptr<FieldDecl>> Fields; ///< Struct fields
// };

}; // namespace phi
