module Main where

class MyClass a

data Foo = Foo
  derive (MyClass)
