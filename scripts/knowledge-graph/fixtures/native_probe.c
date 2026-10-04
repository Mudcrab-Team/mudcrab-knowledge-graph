/* Original synthetic native fixture, MIT OR Apache-2.0. */
#include <stdint.h>
#include <stdio.h>
struct Material { uint32_t flags; float glossiness; const char *texture; };
typedef int (*Dispatch)(const struct Material *);
int material_double_sided(const struct Material *m) { return (m->flags & 16) != 0; }
int material_apply(const struct Material *m, Dispatch dispatch) {
    int enabled = material_double_sided(m);
    return enabled + dispatch(m);
}
int main(void) {
    struct Material m = {16, 80.0f, "synthetic/color.dds"};
    printf("%s: %d\n", m.texture, material_apply(&m, material_double_sided));
    return 0;
}
