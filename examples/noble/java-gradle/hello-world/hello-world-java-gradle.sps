name = "hello-world-java-gradle"
architecture = "any"
summary = """Example Package
This is a short description of the package. It should provide a brief summary
of what the package does and its purpose. The short description should be
limited to a single line."""
conflicts = []
recommends = []
provides = []
suggests = []
depends = []
add_files = [
  "app/lib /usr/lib/hello-world-java-gradle",
  "app/bin /usr/lib/hello-world-java-gradle",
]
add_links=["/usr/lib/hello-world-java-gradle/bin/app /usr/bin/hello-world"]
add_manpages = []
long_doc = """
Example Package
 This is a short description of the package. It should provide a brief summary
 of what the package does and its purpose. The short description should be
 limited to a single line.
 Long Description:
  Example description. If not provided, lintian will fail.
"""