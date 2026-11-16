module OverlapFundep where

class F :: Type -> Type -> Constraint
class F a b | a -> b

instance fIntString :: F Int String

instance fIntBoolean :: F Int Boolean

class G :: Type -> Type -> Constraint
class G a b

instance gIntString :: G Int String

instance gIntBoolean :: G Int Boolean

class H :: Type -> Constraint

instance hInt :: H Int

instance hString :: H String
