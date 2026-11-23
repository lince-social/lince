def park_release:
  length == 1 and (.[0] |
    type == "object" and
    all(.version, .revision, .target; type == "string" and length > 0) and
    (.target | endswith("-linux-gnu")) and
    .optimized == true and
    .features == {ui: true, instinct: true, park: true});

def matching_park_probe($application; $revision; $target):
  ($application | park_release) and length == 1 and (.[0] |
    .probe == "park-responsiveness-v1" and .package == "lince" and
    .revision == $revision and .revision == $application[0].revision and
    .version == $application[0].version and .optimized == true and
    .architecture == "x86_64" and .os == "linux" and
    $target == "x86_64-unknown-linux-gnu" and $application[0].target == $target);
