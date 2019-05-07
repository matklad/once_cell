# Changelog

## 0.2.0

- MSRV is now 1.31.1
- `Lazy::new` and `OnceCell::new` are now const-fns
- `unsync_lazy` and `sync_lazy` macros are removed

## 0.1.8

- update crossbeam-utils to 0.6
- enable bors-ng

## 0.1.7

- cells implement `PartialEq` and `From`
- MSRV is down to 1.24.1
- update `parking_lot` to `0.7.1`

## 0.1.6

- `unsync::OnceCell<T>` is `Clone` if `T` is `Clone`.

## 0.1.5

- No changelog until this point :(
