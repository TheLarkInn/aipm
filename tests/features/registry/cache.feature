@p1 @cache
Feature: Download cache policies
  As a plugin consumer,
  I want to control how the download cache behaves,
  so that I can optimize for speed, freshness, or offline use.

  Scenario: Auto policy caches on first install, serves from cache on second
    Given a git source plugin that has never been installed
    When the user installs it (default Auto policy)
    Then the plugin is cloned and cached at "~/.aipm/cache/entries/<uuid>"
    When the user installs it again
    Then the cached copy is used (no git clone)

  Scenario: CacheOnly policy fails when not cached
    Given a plugin that is not in the download cache
    When the user runs "aipm install --plugin-cache cache-only git:url"
    Then the install fails with "not found in cache (cache-only mode)"

  Scenario: SkipCache policy never reads or writes cache
    When the user runs "aipm install --plugin-cache skip git:url"
    Then no cache entry is created or read
    And a fresh git clone is always performed

  Scenario: ForceRefresh always re-downloads and updates cache
    Given a plugin is already cached
    When the user runs "aipm install --plugin-cache force-refresh git:url"
    Then a fresh git clone is performed
    And the cache entry is replaced with the new content

  Scenario: CacheNoRefresh uses stale cache without re-downloading
    Given a cached plugin that is past its TTL (stale)
    When the user runs "aipm install --plugin-cache no-refresh git:url"
    Then the stale cached copy is used without re-downloading

  Scenario: Cache garbage collection removes old entries
    Given cached plugins that have not been accessed in 30+ days
    When garbage collection runs
    Then stale entries are removed from the cache index
    And their directories are deleted

  Scenario: Installed plugins are exempt from garbage collection
    Given a cached plugin marked as "installed"
    When garbage collection runs
    Then the installed plugin's cache entry is preserved

  Scenario: Per-entry TTL overrides global TTL
    Given a plugin cached with a custom TTL of 1 hour
    When the global TTL is 24 hours
    Then the plugin becomes stale after 1 hour (not 24)
