#include "lince_workflows.h"
#include <string.h>

void lince_custom_run(const cJSON *configuration) {
    const cJSON *workflow = cJSON_GetObjectItemCaseSensitive(configuration, "workflow");
    const cJSON *name = cJSON_GetObjectItemCaseSensitive(workflow, "name");
    const cJSON *output = cJSON_GetObjectItemCaseSensitive(workflow, "output");
    const cJSON *settings = cJSON_GetObjectItemCaseSensitive(workflow, "settings");
    const cJSON *value = cJSON_GetObjectItemCaseSensitive(settings, "value");
    const cJSON *interval = cJSON_GetObjectItemCaseSensitive(settings, "interval_seconds");
    if (!cJSON_IsString(name) || strcmp(name->valuestring, "scalar-demo") != 0 || !cJSON_IsString(output) || strcmp(output->valuestring, "scalar") != 0 || !cJSON_IsString(value) || !lince_decimal_valid(value->valuestring) || !cJSON_IsNumber(interval) || interval->valuedouble < 1 || interval->valuedouble > 86400 || interval->valuedouble != interval->valueint) {
        lince_status_set("custom-workflow-settings-unsupported");
        return;
    }
    cJSON *copy = cJSON_Duplicate(configuration, true);
    if (!copy) { lince_status_set("custom-workflow-allocation-failed"); return; }
    cJSON *scalar = cJSON_Duplicate(settings, true);
    if (!scalar) { cJSON_Delete(copy); lince_status_set("custom-workflow-allocation-failed"); return; }
    cJSON_AddStringToObject(scalar, "kind", "scalar");
    cJSON_ReplaceItemInObjectCaseSensitive(copy, "workflow", scalar);
    lince_scalar_run(copy);
    cJSON_Delete(copy);
}
