idf_component_register(SRCS "camera.c" INCLUDE_DIRS "include" REQUIRES lince_core espressif__esp32-camera json)
target_compile_options(${COMPONENT_LIB} PRIVATE -Werror=extra -Werror=unused-function -Werror=unused-variable -Werror=unused-but-set-variable -Werror=deprecated-declarations -Werror=sign-compare)
