### Changes

- The shell tests link `lib/` into their scratch trees with one helper that replaces a link already there, so an interrupted or repeated run no longer leaves a stray `lib/lib` in the checkout.
