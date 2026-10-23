/* ftlpad_probe: an empty x86_64 program. Run under box64 with ftlpad.so preloaded and
 * FTLPAD_DEBUG set, it shows whether the library loads: an "ftlpad:" log line appears, or
 * box64 says why not.
 */
int main(void)
{
    return 0;
}
