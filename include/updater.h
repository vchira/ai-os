/* AiOS — Kernel Updater */
#ifndef AIOS_UPDATER_H
#define AIOS_UPDATER_H

/* Download kernel from HTTPS URL and write to disk (FAT16 partition).
   Returns 0 on success, negative on error.
   URL must be https://hostname/path format. */
int update_kernel(const char *url);

#endif
