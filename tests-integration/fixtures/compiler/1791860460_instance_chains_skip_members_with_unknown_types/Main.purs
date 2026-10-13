module Main where

type Invalid = Int String

class Fallback :: Type -> Constraint
class Fallback a

instance Fallback (Array Invalid)
else instance Fallback a

needsFallback :: Fallback (Array Int) => Int
needsFallback = 0

usesFallback :: Int
usesFallback = needsFallback

class Unavailable :: Constraint
class Unavailable

class Skipped :: Type -> Constraint
class Skipped a

instance Skipped Invalid
else instance Unavailable => Skipped a

needsSkipped :: Skipped Invalid => Int
needsSkipped = 0

usesSkipped :: Int
usesSkipped = needsSkipped

class SkippedNested :: Type -> Constraint
class SkippedNested a

instance SkippedNested (Array Invalid)
else instance Unavailable => SkippedNested a

needsSkippedNested :: SkippedNested Invalid => Int
needsSkippedNested = 0

usesSkippedNested :: Int
usesSkippedNested = needsSkippedNested
