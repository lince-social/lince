Frequency has two features to be done so we can complete it:

        *Catch Up Sum:*
        When a frequency hasnt been activated for a long time, like for a 1 Day frequency with a next_date stuck
        three months ago, if something references it, every Karma Delivery (60s) will update it to one day closer to tomorrow (2 months 29 days now).

        The catch_up_sum is something that takes all of the possible times the frequency would activate and moves the next_date until it reaches stability.

        catch_up_sum == 0 => dont do anything, just calculate frequency normally one time.

        catch_up_sum => positive, make the next_date jump the number of times the value of catch_up_sum, never jumping if next_date is already in the future.

        In other words: if its 1, its the same as zero, you jump the next_date one time based on the frequency (1 day) and go on.
        If it's two, you jump two times so it would go from 3 months ago to 2 months and 28 days.
        If its negative dont do anything.

        *Days of the Week:*
        There already is a commented try at this in the frequency function. The goal is to make something easy to write to say that it should fall in a day
        of the week. So if the frequency only contains info about jumping every monday and tuesday then the day_of_week would be something like `1, 2` or `12`
        or something else, you who knows.

        If the frequency is `months: 2, day_of_week: 5` it will first jump to the next friday, then jump two months. Or maybe it should first go to the two
        months and then fall on a friday. There should be a mechanism to easily set a prefference between the two behaviors.

        Feel free to refactor this a lot. With those two unfinished parts the frequency will be able to cover many cases, if you have more periodicies in
        mind to cover even more cases please refactor.