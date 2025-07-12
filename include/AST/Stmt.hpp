#include "SrcLocation.hpp"
#include <memory>

#pragma once

class Expr;

class Stmt {
public:
    Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] const SrcLocation& get_location() const { return location; }

protected:
    SrcLocation location;
};

class ReturnStmt : public Stmt {
public:
    ReturnStmt(SrcLocation, std::unique_ptr<Expr>);
    ~ReturnStmt();

    [[nodiscard]] const Expr* get_expr() const;
    void info_dump(int level) const override;

private:
    std::unique_ptr<Expr> expr;
};
