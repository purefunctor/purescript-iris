module Main where

data Proxy (a) = Proxy

identity :: forall (a). a -> a
identity value = value
