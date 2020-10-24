## [0.2.0]
- Add `get_or_insert_with` to masked storage and world APIs
- Allow slice access to DenseVecStorage
- Add a way to fetch zero resources in a generic context.
- Make it possible to join over modified components mutably.
- Re-export `MakeSync` at the top-level.
- Add 'local' versions of `ResourceSet` and `World` for !Send types.
- Only provide rayon dependent things if `rayon` feature is enabled
- Some renames and reorganization to make things less dependent on par_seq
- Use `anymap` for a faster `ResourceSet` implementation
- Change associated type name for `System`
- Add ability to manually mark an entry as modified in a `TrackedStorage`

## [0.1.0]
- Initial release
