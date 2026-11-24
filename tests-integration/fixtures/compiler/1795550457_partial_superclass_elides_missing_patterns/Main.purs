module Main where

data Maybe a = Just a | Nothing

class Partial <= Unsafe

class Unsafe <= VeryUnsafe

class Unrelated

direct :: Partial => Maybe Int -> Int
direct (Just n) = n

viaSuperclass :: Unsafe => Maybe Int -> Int
viaSuperclass (Just n) = n

viaTransitiveSuperclass :: VeryUnsafe => Maybe Int -> Int
viaTransitiveSuperclass (Just n) = n

withoutPartial :: Unrelated => Maybe Int -> Int
withoutPartial (Just n) = n
