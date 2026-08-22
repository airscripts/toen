def parse(command, current_mode):
    words = command.split()

    if not words or words[0] != "$toen":
        return "usage", current_mode

    if len(words) == 1:
        return "chooser", current_mode

    argument = words[1]

    if argument in {"ammodino", "arranda"}:
        return "activated", argument

    if argument == "de":
        return current_mode, current_mode

    if argument == "spengi":
        return "disabled", "spento"

    return "usage", "spento"
