/*

// Substitutions for unification (e.g., T -> i32)
struct Substitution {
  std::unordered_map<std::string, Type> Map;

  bool has(std::string_view Name) const {
    return Map.find(std::string(Name)) != Map.end();
  }
  const Type *lookup(std::string_view Name) const {
    auto It = Map.find(std::string(Name));
    return (It == Map.end()) ? nullptr : &It->second;
  }
  void bind(std::string Name, Type Ty) {
    Map.emplace(std::move(Name), std::move(Ty));
  }
};

// Apply substitution to a Type (capture shared sub-structure via shared_ptr)
inline Type apply(const Substitution &S, const Type &T) {
  struct V {
    const Substitution &S;
    Type operator()(PrimitiveKind K) const { return Type::makePrimitive(K); }
    Type operator()(const TypeVariable &V) const {
      if (const Type *Found = S.lookup(V.Name))
        return *Found;
      return Type::makeVariable(V.Name);
    }
    Type operator()(const CustomType &C) const {
      return Type::makeCustom(C.Name);
    }
    Type operator()(const ReferenceType &R) const {
      return Type::makeReference(apply(S, *R.Pointee));
    }
    Type operator()(const PointerType &P) const {
      return Type::makePointer(apply(S, *P.Pointee));
    }
    Type operator()(const GenericType &G) const {
      std::vector<Type> Args;
      Args.reserve(G.TypeArguments.size());
      for (const auto &A : G.TypeArguments)
        Args.push_back(apply(S, A));
      return Type::makeGeneric(G.Name, std::move(Args));
    }
    Type operator()(const FunctionType &F) const {
      std::vector<Type> Ps;
      Ps.reserve(F.Parameters.size());
      for (const auto &P : F.Parameters)
        Ps.push_back(apply(S, P));
      return Type::makeFunction(std::move(Ps), apply(S, *F.ReturnType));
    }
  };
  return std::visit(V{S}, T.node());
}

// Occurs check: does variable occur in type (to avoid infinite types)?
inline bool occurs(std::string_view Var, const Type &Ty) {
  struct V {
    std::string_view Var;
    bool operator()(PrimitiveKind) const { return false; }
    bool operator()(const TypeVariable &Vv) const { return Vv.Name == Var; }
    bool operator()(const CustomType &) const { return false; }
    bool operator()(const ReferenceType &R) const {
      return occurs(Var, *R.Pointee);
    }
    bool operator()(const PointerType &P) const {
      return occurs(Var, *P.Pointee);
    }
    bool operator()(const GenericType &G) const {
      for (const auto &A : G.TypeArguments)
        if (occurs(Var, A))
          return true;
      return false;
    }
    bool operator()(const FunctionType &F) const {
      for (const auto &P : F.Parameters)
        if (occurs(Var, P))
          return true;
      return occurs(Var, *F.ReturnType);
    }
  };
  return std::visit(V{Var}, Ty.node());
}

// Unification: pattern (may contain variables) vs concrete.
// Produces substitution mapping pattern variables to concrete types.
inline bool unify(const Type &Pattern, const Type &Concrete,
                  Substitution &Out) {
  Type P = apply(Out, Pattern);
  Type C = apply(Out, Concrete);

  if (P == C)
    return true;

  // helpers
  auto VarCase = [&](const TypeVariable &Tv, const Type &Other) -> bool {
    if (occurs(Tv.Name, Other))
      return false; // occurs check
    Out.bind(Tv.Name, Other);
    return true;
  };

  const auto &PN = P.node();
  const auto &CN = C.node();

  if (auto *Pv = std::get_if<TypeVariable>(&PN))
    return VarCase(*Pv, C);
  if (auto *Cv = std::get_if<TypeVariable>(&CN))
    return VarCase(*Cv, P);

  if (auto *Pp = std::get_if<PrimitiveKind>(&PN)) {
    if (auto *Cp = std::get_if<PrimitiveKind>(&CN))
      return *Pp == *Cp;
    return false;
  }
  if (auto *Pc = std::get_if<CustomType>(&PN)) {
    if (auto *Cc = std::get_if<CustomType>(&CN))
      return Pc->Name == Cc->Name;
    return false;
  }
  if (auto *Pr = std::get_if<ReferenceType>(&PN)) {
    if (auto *Cr = std::get_if<ReferenceType>(&CN))
      return unify(*Pr->Pointee, *Cr->Pointee, Out);
    return false;
  }
  if (auto *Pp2 = std::get_if<PointerType>(&PN)) {
    if (auto *Cp2 = std::get_if<PointerType>(&CN))
      return unify(*Pp2->Pointee, *Cp2->Pointee, Out);
    return false;
  }
  if (auto *Pg = std::get_if<GenericType>(&PN)) {
    if (auto *Cg = std::get_if<GenericType>(&CN)) {
      if (Pg->Name != Cg->Name ||
          Pg->TypeArguments.size() != Cg->TypeArguments.size())
        return false;
      for (size_t I = 0; I < Pg->TypeArguments.size(); ++I)
        if (!unify(Pg->TypeArguments[I], Cg->TypeArguments[I], Out))
          return false;
      return true;
    }
    return false;
  }
  if (auto *Pf = std::get_if<FunctionType>(&PN)) {
    if (auto *Cf = std::get_if<FunctionType>(&CN)) {
      if (Pf->Parameters.size() != Cf->Parameters.size())
        return false;
      for (size_t I = 0; I < Pf->Parameters.size(); ++I)
        if (!unify(Pf->Parameters[I], Cf->Parameters[I], Out))
          return false;
      return unify(*Pf->ReturnType, *Cf->ReturnType, Out);
    }
    return false;
  }

  return false;
}

//===----------------------------------------------------------------------===//
// TypeClass System
//   - TypeClass (name + method names)
//   - Constraint (Class + TypePattern)
//   - Instance (Head type + constraints + methods dict)
//   - InstanceRegistry (add/resolve with unification + constraint solving)
//===----------------------------------------------------------------------===//

struct TypeClass {
  std::string Name;
  std::vector<std::string> MethodNames; // ordered, optional (for tooling)
};

struct Constraint {
  const TypeClass *Class;
  Type Wanted; // usually contains type variables
};

struct Instance {
  const TypeClass *Class;
  Type Head; // instance head, e.g., Eq<Vector<T>>
  std::vector<Constraint> Constraints;

  // Method dictionary: name -> callable
  // Convention: callables receive arguments as std::any and return std::any.
  // A real system would enforce signatures; kept dynamic here to stay generic.
  std::unordered_map<std::string,
                     std::function<std::any(const std::vector<std::any> &)>>
      Methods;
};

// Hash for (TypeClass*, stringified Type) — simple but effective for caching.
struct CacheKey {
  const TypeClass *Class;
  std::string TypeStr;
  bool operator==(const CacheKey &Other) const noexcept {
    return Class == Other.Class && TypeStr == Other.TypeStr;
  }
};
struct CacheKeyHash {
  std::size_t operator()(const CacheKey &K) const noexcept {
    return std::hash<const void *>()(K.Class) ^
           std::hash<std::string>()(K.TypeStr);
  }
};

class InstanceRegistry {
public:
  // Add a class for completeness; not strictly required
  void addClass(TypeClass C) { Classes.emplace_back(std::move(C)); }

  // Store instance; keeps stable storage.
  Instance &addInstance(Instance I) {
    Instances.emplace_back(std::make_unique<Instance>(std::move(I)));
    return *Instances.back();
  }

  // Resolve (Class, Type) -> concrete Instance* with constraints satisfied.
  // Throws std::runtime_error on failure/ambiguity.
  const Instance *resolve(const TypeClass &Class, const Type &Ty) {
    CacheKey Key{&Class, Ty.toString()};
    if (auto It = Cache.find(Key); It != Cache.end())
      return It->second;

    // Avoid infinite recursion for recursive constraints
    RecursionGuard.insert(Key);

    const Instance *Best = nullptr;
    for (const auto &Ptr : Instances) {
      const Instance &Cand = *Ptr;
      if (Cand.Class != &Class)
        continue;

      Substitution Subst;
      if (!unify(Cand.Head, Ty, Subst))
        continue;

      // Discharge constraints under the substitution
      bool Ok = true;
      std::unordered_map<std::string, const Instance *> ResolvedDeps;
      for (const auto &Con : Cand.Constraints) {
        Type Wanted = apply(Subst, Con.Wanted);
        CacheKey DepKey{Con.Class, Wanted.toString()};
        if (RecursionGuard.count(DepKey)) {
          Ok = false;
          break;
        } // cycle
        const Instance *Dep = resolve(*Con.Class, Wanted);
        if (!Dep) {
          Ok = false;
          break;
        }
        ResolvedDeps.emplace(Con.Class->Name + ":" + Wanted.toString(), Dep);
      }
      if (!Ok)
        continue;

      if (Best) {
        // Ambiguity: two different instances match
        if (Best != &Cand)
          throw std::runtime_error("Ambiguous instance for " + Class.Name +
                                   " " + Ty.toString());
      }
      Best = &Cand;
    }

    RecursionGuard.erase(Key);
    if (!Best)
      throw std::runtime_error("No instance found for " + Class.Name + " " +
                               Ty.toString());

    Cache.emplace(Key, Best);
    return Best;
  }

private:
  std::vector<TypeClass> Classes;
  std::vector<std::unique_ptr<Instance>> Instances;

  std::unordered_set<CacheKey, CacheKeyHash> RecursionGuard;
  std::unordered_map<CacheKey, const Instance *, CacheKeyHash> Cache;
};

//===----------------------------------------------------------------------===//
// Convenience helpers for method invocation
//   - These make using the dictionary less clunky for common signatures.
//   - For production, you’d autogenerate these from MethodNames/Signatures.
//===----------------------------------------------------------------------===//

// Call a unary method returning R:  R f(x)
template <typename R, typename X>
R callUnary(const Instance &Inst, std::string_view Method, X &&Arg) {
  auto It = Inst.Methods.find(std::string(Method));
  if (It == Inst.Methods.end())
    throw std::runtime_error("Method not found");
  std::vector<std::any> Args;
  Args.emplace_back(std::forward<X>(Arg));
  std::any Ret = It->second(Args);
  return std::any_cast<R>(Ret);
}

// Call a binary method returning R:  R f(x, y)
template <typename R, typename X, typename Y>
R callBinary(const Instance &Inst, std::string_view Method, X &&A, Y &&B) {
  auto It = Inst.Methods.find(std::string(Method));
  if (It == Inst.Methods.end())
    throw std::runtime_error("Method not found");
  std::vector<std::any> Args;
  Args.emplace_back(std::forward<X>(A));
  Args.emplace_back(std::forward<Y>(B));
  std::any Ret = It->second(Args);
  return std::any_cast<R>(Ret);
}

//===----------------------------------------------------------------------===//
// Demo / Reference Setup
//   - Defines Eq and Show classes
//   - Instances: Eq for i32, string, Vector<T> (requires Eq<T>)
//                Show for i32, string, Vector<T> (requires Show<T>)
//   - Resolves and calls methods to show it works.
//===----------------------------------------------------------------------===//

inline void installEqShowExamples(InstanceRegistry &Registry) {
  // Define classes
  TypeClass EqCls{"Eq", {"equal"}};
  TypeClass ShowCls{"Show", {"show"}};
  Registry.addClass(EqCls);
  Registry.addClass(ShowCls);

  // eq i32
  {
    Instance Inst;
    Inst.Class = &EqCls;
    Inst.Head = Type::makePrimitive(PrimitiveKind::I32);
    Inst.Methods["equal"] = [](const std::vector<std::any> &Args) -> std::any {
      const int &A = std::any_cast<const int &>(Args[0]);
      const int &B = std::any_cast<const int &>(Args[1]);
      return std::any(A == B);
    };
    Registry.addInstance(std::move(Inst));
  }

  // eq string
  {
    Instance Inst;
    Inst.Class = &EqCls;
    Inst.Head = Type::makePrimitive(PrimitiveKind::String);
    Inst.Methods["equal"] = [](const std::vector<std::any> &Args) -> std::any {
      const std::string &A = std::any_cast<const std::string &>(Args[0]);
      const std::string &B = std::any_cast<const std::string &>(Args[1]);
      return std::any(A == B);
    };
    Registry.addInstance(std::move(Inst));
  }

  // eq Vector<T>  :-  Eq<T>
  {
    Instance Inst;
    Inst.Class = &EqCls;
    Type Tvar = Type::makeVariable("T");
    Inst.Head = Type::makeGeneric("Vector", {Tvar});
    Inst.Constraints.push_back(Constraint{&EqCls, Tvar});
    Inst.Methods["equal"] = [](const std::vector<std::any> &Args) -> std::any {
      const auto &A = std::any_cast<const std::vector<std::any> &>(Args[0]);
      const auto &B = std::any_cast<const std::vector<std::any> &>(Args[1]);
      if (A.size() != B.size())
        return std::any(false);
      for (size_t I = 0; I < A.size(); ++I) {
        // Elements must already be bools from previous comparisons if you were
        // composing; here we just compare by value where possible.
        if (A[I].type() != B[I].type())
          return std::any(false);
        if (A[I].type() == typeid(int)) {
          if (std::any_cast<int>(A[I]) != std::any_cast<int>(B[I]))
            return std::any(false);
        } else if (A[I].type() == typeid(std::string)) {
          if (std::any_cast<std::string>(A[I]) !=
              std::any_cast<std::string>(B[I]))
            return std::any(false);
        } else {
          // For a real interpreter, you’d pass element Eq dictionary in
          // closure.
          return std::any(false);
        }
      }
      return std::any(true);
    };
    Registry.addInstance(std::move(Inst));
  }

  // show i32
  {
    Instance Inst;
    Inst.Class = &ShowCls;
    Inst.Head = Type::makePrimitive(PrimitiveKind::I32);
    Inst.Methods["show"] = [](const std::vector<std::any> &Args) -> std::any {
      const int &A = std::any_cast<const int &>(Args[0]);
      return std::any(std::to_string(A));
    };
    Registry.addInstance(std::move(Inst));
  }

  // show string
  {
    Instance Inst;
    Inst.Class = &ShowCls;
    Inst.Head = Type::makePrimitive(PrimitiveKind::String);
    Inst.Methods["show"] = [](const std::vector<std::any> &Args) -> std::any {
      const std::string &A = std::any_cast<const std::string &>(Args[0]);
      return std::any(A);
    };
    Registry.addInstance(std::move(Inst));
  }

  // show Vector<T> :- Show<T>
  {
    Instance Inst;
    Inst.Class = &ShowCls;
    Type Tvar = Type::makeVariable("T");
    Inst.Head = Type::makeGeneric("Vector", {Tvar});
    Inst.Constraints.push_back(Constraint{&ShowCls, Tvar});
    Inst.Methods["show"] = [](const std::vector<std::any> &Args) -> std::any {
      const auto &Vec = std::any_cast<const std::vector<std::any> &>(Args[0]);
      std::ostringstream Oss;
      Oss << "[";
      for (size_t I = 0; I < Vec.size(); ++I) {
        const std::any &E = Vec[I];
        if (I)
          Oss << ", ";
        if (E.type() == typeid(int))
          Oss << std::any_cast<int>(E);
        else if (E.type() == typeid(std::string))
          Oss << std::any_cast<std::string>(E);
        else
          Oss << "<?>";
      }
      Oss << "]";
      return std::any(Oss.str());
    };
    Registry.addInstance(std::move(Inst));
  }
}

//===----------------------------------------------------------------------===//
// Utility predicates (optional – use or extend as needed)
//===----------------------------------------------------------------------===//

inline bool isIntegerType(const Type &Ty) {
  if (auto P = std::get_if<PrimitiveKind>(&Ty.node())) {
    return Type::isInteger(*P);
  }
  return false;
}
inline bool isSignedIntegerType(const Type &Ty) {
  if (auto P = std::get_if<PrimitiveKind>(&Ty.node()))
    return Type::isSignedInteger(*P);
  return false;
}
inline bool isUnsignedIntegerType(const Type &Ty) {
  if (auto P = std::get_if<PrimitiveKind>(&Ty.node()))
    return Type::isUnsignedInteger(*P);
  return false;
}
inline bool isFloatType(const Type &Ty) {
  if (auto P = std::get_if<PrimitiveKind>(&Ty.node()))
    return Type::isFloat(*P);
  return false;
}
inline bool isNumericType(const Type &Ty) {
  if (auto P = std::get_if<PrimitiveKind>(&Ty.node()))
    return Type::isInteger(*P) || Type::isFloat(*P);
  return false;
}

} // namespace phi

//===----------------------------------------------------------------------===//
// Demo program (define PHI_TYPECLASS_DEMO to build and run)
//===----------------------------------------------------------------------===//
#if defined(PHI_TYPECLASS_DEMO)
int main() {
  using namespace phi;

  InstanceRegistry Registry;
  installEqShowExamples(Registry);

  Type I32 = Type::makePrimitive(PrimitiveKind::I32);
  Type Str = Type::makePrimitive(PrimitiveKind::String);
  Type VecI32 = Type::makeGeneric("Vector", {I32});
  Type VecStr = Type::makeGeneric("Vector", {Str});

  // Resolve Eq for i32
  {
    const TypeClass EqCls{"Eq", {"equal"}};
    const Instance *EqI32 = Registry.resolve(EqCls, I32);
    bool R = callBinary<bool>(*EqI32, "equal", 42, 42);
    std::cout << "Eq i32: 42 == 42 ? " << std::boolalpha << R << "\n";
  }

  // Resolve Show for Vector<string>
  {
    const TypeClass ShowCls{"Show", {"show"}};
    const Instance *ShowVecStr = Registry.resolve(ShowCls, VecStr);

    std::vector<std::any> Data = {std::string("hi"), std::string("phi")};
    std::string S = callUnary<std::string>(*ShowVecStr, "show", Data);
    std::cout << "Show [\"hi\",\"phi\"] => " << S << "\n";
  }

  // Try resolving Eq for Vector<i32>
  {
    const TypeClass EqCls{"Eq", {"equal"}};
    const Instance *EqVecI32 = Registry.resolve(EqCls, VecI32);

    std::vector<std::any> A = {1, 2, 3};
    std::vector<std::any> B = {1, 2, 3};
    bool R = callBinary<bool>(*EqVecI32, "equal", A, B);
    std::cout << "Eq Vector<i32>: [1,2,3] == [1,2,3] ? " << std::boolalpha << R
              << "\n";
  }

  return 0;
}
#endif

*/
