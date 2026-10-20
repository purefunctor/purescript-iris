module Main where

literal = { name: "Iris", nested: { value: 42 } }
--          $             $         $

update = literal { name = 'I', nested { value = true } }
--                 $           $        $

binder { name: label, nested: { value } } = label
--       $            $         $
