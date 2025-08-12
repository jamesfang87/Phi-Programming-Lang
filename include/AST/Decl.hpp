#pragma once

#include <memory>
#include <optional>
#include <print>
#include <stdexcept>
#include <string>
#include <unordered_map>
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
  enum class Kind : uint8_t {
    VarDecl,
    ParamDecl,
    FunDecl,
    StructDecl,
    FieldDecl
  };

  Decl(Kind K, SrcLocation Location, std::string Id, Type DeclType)
      : Kind(K), Location(std::move(Location)), Id(std::move(Id)),
        DeclType(std::move(DeclType)) {}

  virtual ~Decl() = default;

  [[nodiscard]] Kind getKind() const { return Kind; }
  [[nodiscard]] SrcLocation getLocation() const { return Location; }
  [[nodiscard]] std::string getId() const { return Id; }
  [[nodiscard]] Type getType() {
    if (!DeclType.has_value()) {
      std::println("ERROR: Attempting to get type of unresolved declaration "
                   "'{}' (kind: {})",
                   Id, static_cast<int>(Kind));
      throw std::runtime_error("Unresolved declaration type");
    }
    return *DeclType;
  }
  [[nodiscard]] std::optional<Type> getOptionalType() const { return DeclType; }
  [[nodiscard]] virtual bool isConst() const = 0;

  virtual void emit(int level) const = 0;

protected:
  Kind Kind;                    ///< Kind of declaration
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
   * @param Location Source location of declaration
   * @param Id Variable name
   * @param DeclType
   * @param IsConst
   * @param Init
   */
  VarDecl(SrcLocation Location, std::string Id, Type DeclType,
          const bool IsConst, std::unique_ptr<Expr> Init)
      : Decl(Kind::VarDecl, std::move(Location), std::move(Id),
             std::move(DeclType)),
        IsConst(IsConst), Init(std::move(Init)) {}

  [[nodiscard]] bool isConst() const override { return IsConst; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }
  [[nodiscard]] Expr &getInit() const { return *Init; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::VarDecl; }

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
   * @param Location
   * @param Id
   * @param DeclType
   * @param IsConst
   */
  ParamDecl(SrcLocation Location, std::string Id, Type DeclType, bool IsConst)
      : Decl(Kind::ParamDecl, std::move(Location), std::move(Id),
             std::move(DeclType)),
        IsConst(IsConst) {}

  [[nodiscard]] bool isConst() const override { return IsConst; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::ParamDecl; }

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
      : Decl(Kind::FunDecl, std::move(Location), std::move(Id),
             std::move(ReturnType)),
        Params(std::move(Params)), Block(std::move(BlockPtr)) {}

  [[nodiscard]] bool isConst() const override { return false; }
  [[nodiscard]] Type getReturnTy() const {
    if (!DeclType.has_value()) {
      std::println(
          "ERROR: Attempting to get return type of unresolved function '{}'",
          Id);
      throw std::runtime_error("Unresolved function return type");
    }
    return DeclType.value();
  }
  [[nodiscard]] Block &getBlock() const { return *Block; }
  [[nodiscard]] std::vector<std::unique_ptr<ParamDecl>> &getParams() {
    return Params;
  }
  void setBlock(std::unique_ptr<Block> BPtr) { Block = std::move(BPtr); }

  static bool classof(const Decl *D) { return D->getKind() == Kind::FunDecl; }

  void emit(int level) const override;

private:
  std::vector<std::unique_ptr<ParamDecl>> Params; ///< Function parameters
  std::unique_ptr<Block> Block;                   ///< Function body statements
};

class FieldDecl final : public Decl {
public:
  /**
   * @brief Constructs a FieldDecl node
   * @param location Source location of field
   * @param identifier Field name
   * @param type Field type
   * @param isConst Whether the field is constant
   */
  FieldDecl(SrcLocation Location, std::string Id, Type Type,
            std::unique_ptr<Expr> Init, bool IsPrivate)
      : Decl(Kind::FieldDecl, std::move(Location), std::move(Id),
             std::move(Type)),
        IsPrivate(IsPrivate), Init(std::move(Init)) {}

  [[nodiscard]] bool isPrivate() const { return IsPrivate; }
  [[nodiscard]] bool isConst() const override { return false; }
  [[nodiscard]] bool hasInit() const { return Init != nullptr; }

  [[nodiscard]] Expr &getInit() const { return *Init; }

  static bool classof(const Decl *D) { return D->getKind() == Kind::FieldDecl; }

  void emit(int level) const override;

private:
  bool IsPrivate;
  std::unique_ptr<Expr> Init;
};

class StructDecl final : public Decl {
public:
  /**
   * @brief Constructs a StructDecl node
   * @param Location Source location of struct
   * @param Id Struct name
   * @param Fields List of struct fields
   */
  StructDecl(SrcLocation Location, std::string Id,
             std::vector<FieldDecl> Fields, std::vector<FunDecl> Methods)
      : Decl(Kind::StructDecl, std::move(Location), Id, Type(std::move(Id))),
        Fields(std::move(Fields)), Methods(std::move(Methods)) {
    this->FieldMap.reserve(this->Fields.size());
    this->MethodMap.reserve(this->Methods.size());
    for (auto &field : this->Fields) {
      this->FieldMap[field.getId()] = &field;
    }
    for (auto &method : this->Methods) {
      this->MethodMap[method.getId()] = &method;
    }
  }

  [[nodiscard]] bool isConst() const override { return false; }

  [[nodiscard]] std::vector<FieldDecl> &getFields() { return Fields; }
  [[nodiscard]] std::vector<FunDecl> &getMethods() { return Methods; }

  [[nodiscard]] FieldDecl *getField(const std::string &Id) {
    if (!FieldMap.contains(Id))
      return nullptr;

    return FieldMap[Id];
  }

  [[nodiscard]] FunDecl *getMethod(const std::string &Id) {
    if (!MethodMap.contains(Id))
      return nullptr;

    return MethodMap[Id];
  }

  static bool classof(const Decl *D) {
    return D->getKind() == Kind::StructDecl;
  }

  void emit(int level) const override;

private:
  std::vector<FieldDecl> Fields; ///< Struct fields
  std::vector<FunDecl> Methods;  ///< Struct methods

  std::unordered_map<std::string, FieldDecl *> FieldMap;
  std::unordered_map<std::string, FunDecl *> MethodMap;
};

}; // namespace phi
