# Smithay capture helper backport

The two capture modules originate in [Smithay commit 94687f51cb7a4c57559f5f5f72aa1c68a60ea940](https://github.com/Smithay/smithay/commit/94687f51cb7a4c57559f5f5f72aa1c68a60ea940), merged through [Smithay pull request 1902](https://github.com/Smithay/smithay/pull/1902). They implement the standard ext image capture source and image copy capture protocols.

The upstream implementation was contributed by Greg Lamberson and derives from COSMIC's capture implementation by Victoria Brekenfeld and Ian Douglas Scott at System76. Smithay distributes it under the MIT license included in LICENSE.txt.

This isolated backport reuses the repository's pinned Smithay 0.7 types. It avoids upgrading the compositor, renderer and window backend together. Source documentation comments are omitted to follow the repository's code conventions. Local lifecycle, resource bounds and protocol validation corrections are maintained here; Park's rendering, permission and session policy remain in the Park crate. Compilation and protocol acceptance are pending.
