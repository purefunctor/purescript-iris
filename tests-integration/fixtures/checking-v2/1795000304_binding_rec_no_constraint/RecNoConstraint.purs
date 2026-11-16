module RecNoConstraint where

-- Control: a recursive group without constraints generalizes each member.
f x y = g y x
g x y = f y x
